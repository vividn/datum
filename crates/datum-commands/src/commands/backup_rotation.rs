//! `datum backup --to DIR` — a Tower of Hanoi rotation of backups kept in
//! one directory (da-d5cmj), a step at a time. `datum backup --auto --every
//! D [--slots N]` records the period and cap in DIR as it installs the timer
//! that takes the steps, and each step reads them there (da-tsna7): a step by
//! hand into a DIR holding no rotation is refused, so a backup by hand starts
//! none.
//!
//! Slot 0 is the first backup ever taken in DIR for a db, and nothing
//! overwrites it. Every later backup is keyed by its period, `p = floor((now
//! − slot 0's time) / every)`, and lands in slot `1 + trailing_zeros(p)`,
//! clamped to `--slots`: a re-run in the same period replaces its own slot,
//! and a period nobody ran is simply absent. Each slot is one gzipped JSONL
//! file — the ordinary backup format, so `db restore` reads it — and
//! `index.json` says what is there. Each db's rotation lives in a
//! subdirectory of its own, `DIR/<label>/`, so two dbs' runs never share an
//! index (da-o6fvk); a db the `index.json` at DIR's top lists is a rotation
//! begun flat before that, and is rotated where it is, as no other db is. The index is written after the slot
//! file is whole and the slot's previous file removed only after that, so a
//! run killed anywhere leaves an index naming whole files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use datum_core::db::Db;
use datum_parse::time::{format_human_duration, DurationPrecision};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::backup_restore::{create_private_file, derive_source_db, write_jsonl_gz, BackupHeader};
use crate::args::{BackupRotationArgs, RotationAction, ShowFlag};

#[cfg(feature = "net")]
use super::backup_daemon::would_install;

/// A build with no timers (datum.day's) has none to say it would install.
#[cfg(not(feature = "net"))]
fn would_install(
    _: Option<String>,
    _: i64,
    _: Option<u32>,
    _: Option<&str>,
    _: bool,
) -> Result<(String, String, String)> {
    anyhow::bail!("no backup timer in this build")
}

const INDEX_FILE: &str = "index.json";
const INDEX_VERSION: u32 = 1;

/// `index.json`: per db, the rotation's settings and the backups it holds;
/// at DIR's top, `all`, the settings of the timer backing up every database
/// into DIR, which each of its steps takes (da-tsna7).
#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    #[serde(default)]
    dbs: BTreeMap<String, Rotation>,
    #[serde(default)]
    all: Option<Schedule>,
}

/// A rotation's period and its cap on the slots, slot 0 not counted.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub(crate) struct Schedule {
    pub every_secs: i64,
    pub slots: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Rotation {
    #[serde(default)]
    every_secs: i64,
    /// The cap, slot 0 not counted; `None` grows with time.
    slots: Option<u32>,
    #[serde(default)]
    backups: Vec<Slot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Slot {
    slot: u32,
    period: u64,
    /// Relative to the directory.
    file: String,
    backup_time: DateTime<Utc>,
    source_db: String,
    entry_count: usize,
    size: u64,
}

/// The slot period `p` lands in: 0 is slot 0's own, the rest go to
/// `1 + trailing_zeros(p)`, clamped to `cap`.
pub fn slot_for(p: u64, cap: Option<u32>) -> u32 {
    if p == 0 {
        return 0;
    }
    let slot = 1 + p.trailing_zeros();
    cap.map_or(slot, |c| slot.min(c))
}

/// `(slot, period)` for every slot a rotation holds once periods
/// `0..=latest` have all run: the newest period [`slot_for`] sends there.
pub fn plan(latest: u64, cap: Option<u32>) -> Vec<(u32, u64)> {
    let mut out = vec![(0, 0)];
    for k in 0..64 {
        let slot = k + 1;
        let mut m = latest >> k;
        if m == 0 {
            break;
        }
        if cap == Some(slot) {
            // The top slot takes every period with k or more trailing zeros.
            out.push((slot, m << k));
            break;
        }
        // Exactly k trailing zeros: the newest odd multiple of 2^k.
        if m.is_multiple_of(2) {
            m -= 1;
        }
        out.push((slot, m << k));
    }
    out
}

pub fn cmd_backup_rotation(args: BackupRotationArgs, cli_db: Option<&str>, db: &Db) -> Result<()> {
    let now = Utc::now();
    let mut failed = Vec::new();
    let (doc, text) = match &args.action {
        RotationAction::To(dir) if args.all => {
            anyhow::ensure!(
                cli_db.is_none(),
                "--all or --db, not both: --all backs up every database datum knows"
            );
            rotate_all(Path::new(dir), now, &mut failed)?
        }
        RotationAction::To(dir) => {
            let source_db = derive_source_db(cli_db);
            rotate(db, Path::new(dir), &source_db, None, now)?
        }
        RotationAction::List(dir) => list(Path::new(dir), now)?,
        RotationAction::Plan { span, to } => {
            let every = args.every.unwrap_or(datum_parse::cli::BACKUP_EVERY_SECS);
            let set_up = would_install(to.clone(), every, args.slots, cli_db, args.all);
            let (mut doc, mut text) = match span {
                Some(span) => plan_report(every, args.slots, *span),
                None => (
                    json!({ "every_secs": every, "slots": args.slots }),
                    String::new(),
                ),
            };
            match set_up {
                Ok((name, to, what)) => {
                    doc["timer"] = json!(format!("{name}.timer"));
                    doc["to"] = json!(to);
                    text = format!(
                        "would install the systemd user timer {name}.timer: {what}\n{text}"
                    );
                }
                Err(e) => text = format!("{e:#}\n{text}"),
            }
            (doc, text)
        }
    };
    match args.show {
        ShowFlag::None => {}
        ShowFlag::Json => println!("{doc}"),
        _ => print!("{text}"),
    }
    anyhow::ensure!(failed.is_empty(), "no backup of {}", failed.join("; "));
    Ok(())
}

/// `--all`: every database datum knows at this run, each into its own
/// rotation in `dir`, so one made since the install is taken and one deleted
/// drops out, at the period and cap DIR's top records for every database;
/// a DIR recording none is refused. One that fails is named in `failed`, the
/// rest taken (da-bw7ur).
fn rotate_all(
    dir: &Path,
    now: DateTime<Utc>,
    failed: &mut Vec<String>,
) -> Result<(serde_json::Value, String)> {
    // Refused whole, as one database's step into a DIR holding none is.
    let Some(schedule) = read_index(dir)?.all else {
        anyhow::bail!(
            "{} holds no rotation of every database: `datum backup --to {} --auto --all` starts one",
            dir.display(),
            dir.display()
        );
    };
    let (mut docs, mut text) = (serde_json::Map::new(), String::new());
    // A registry entry whose file is gone is no database, and is not made one.
    for (name, path) in crate::config::known_dbs()
        .into_iter()
        .filter(|(_, p)| p.exists())
    {
        let opened = match Db::writable(&path) {
            true => Db::open(&path.to_string_lossy()),
            false => Db::open_to_read(&path.to_string_lossy()),
        };
        match (opened.map_err(anyhow::Error::from))
            .and_then(|db| rotate(&db, dir, &name, Some(schedule), now))
        {
            Ok((doc, t)) => {
                docs.insert(name.clone(), doc);
                text += &format!("{name}: {t}");
            }
            Err(e) => failed.push(format!("{name} ({e:#})")),
        }
    }
    Ok((docs.into(), text))
}

/// Where in DIR `source_db`'s rotation lives: DIR itself for one begun flat
/// before the subdirectories, else `DIR/<label>/`.
fn home(dir: &Path, source_db: &str) -> Result<PathBuf> {
    Ok(match read_index(dir)?.dbs.contains_key(source_db) {
        true => PathBuf::new(),
        false => PathBuf::from(label(source_db)),
    })
}

/// The period and cap a scheduled rotation takes, recorded in DIR as its
/// timer is installed, for each step to read (da-tsna7): `source_db`'s own,
/// or with `None` every database's, at DIR's top. A rotation already there
/// keeps its backups and takes the new settings, as a step with another
/// `--every` did.
#[cfg_attr(not(feature = "net"), allow(dead_code))]
pub(crate) fn set_schedule(dir: &Path, source_db: Option<&str>, schedule: Schedule) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("Creating {}", dir.display()))?;
    let Some(source_db) = source_db else {
        let mut index = read_index(dir)?;
        index.all = Some(schedule);
        return write_index(dir, &index);
    };
    let dir = &dir.join(home(dir, source_db)?);
    std::fs::create_dir_all(dir).with_context(|| format!("Creating {}", dir.display()))?;
    let mut index = read_index(dir)?;
    let rotation = index
        .dbs
        .entry(source_db.to_string())
        .or_insert_with(|| Rotation {
            every_secs: 0,
            slots: None,
            backups: Vec::new(),
        });
    (rotation.every_secs, rotation.slots) = (schedule.every_secs, schedule.slots);
    write_index(dir, &index)
}

/// One step of `source_db`'s rotation in `dir`, at `schedule`, or with
/// `None` at the period and cap its index records.
fn rotate(
    db: &Db,
    dir: &Path,
    source_db: &str,
    schedule: Option<Schedule>,
    now: DateTime<Utc>,
) -> Result<(serde_json::Value, String)> {
    let top = dir;
    let sub = home(dir, source_db)?;
    let named = |f: &str| sub.join(f).to_string_lossy().into_owned();
    let dir = &dir.join(&sub);
    let mut index = read_index(dir)?;
    let held = (index.dbs.get(source_db))
        .filter(|r| r.every_secs > 0)
        .map(|r| Schedule {
            every_secs: r.every_secs,
            slots: r.slots,
        });
    let Some(Schedule {
        every_secs: every,
        slots: cap,
    }) = schedule.or(held)
    else {
        anyhow::bail!(
            "{} holds no rotation of {source_db}: `datum backup --to {} --auto` starts one, \
             or `datum backup <PATH>` writes one file",
            top.display(),
            top.display()
        );
    };
    std::fs::create_dir_all(dir).with_context(|| format!("Creating {}", dir.display()))?;
    let rotation = index
        .dbs
        .entry(source_db.to_string())
        .or_insert_with(|| Rotation {
            every_secs: every,
            slots: cap,
            backups: Vec::new(),
        });
    let period = match rotation.backups.iter().find(|b| b.slot == 0) {
        None => 0,
        Some(first) => match (now - first.backup_time).num_seconds() / every {
            p if p > 0 => p as u64,
            // Slot 0's own period, or a clock that went back before it.
            _ => {
                let until = first.backup_time + chrono::Duration::seconds(every);
                let doc = json!({ "slot": null, "until": until, "written": [], "removed": [] });
                let text = match now < first.backup_time {
                    true => format!(
                        "The clock is behind slot 0's backup ({}): nothing written.\n",
                        first.backup_time.to_rfc3339()
                    ),
                    false => format!(
                        "Slot 0 holds this period, until {}: nothing written.\n",
                        until.to_rfc3339()
                    ),
                };
                return Ok((doc, text));
            }
        },
    };
    rotation.every_secs = every;
    rotation.slots = cap;
    let slot = slot_for(period, cap);
    let written = write_slot(db, dir, source_db, slot, period, now)?;
    let old = rotation
        .backups
        .iter()
        .position(|b| b.slot == slot)
        .map(|i| rotation.backups.remove(i));
    rotation.backups.push(written.clone());
    rotation.backups.sort_by_key(|b| b.slot);
    write_index(dir, &index)?;
    let removed: Vec<String> = old
        .filter(|o| o.file != written.file)
        .map(|o| {
            let _ = std::fs::remove_file(dir.join(&o.file));
            named(&o.file)
        })
        .into_iter()
        .collect();

    let text = format!(
        "✓ Backup: {} entries → slot {slot} (period {period}) {}\n{}",
        written.entry_count,
        dir.join(&written.file).display(),
        removed
            .iter()
            .map(|f| format!("  replaced {f}\n"))
            .collect::<String>()
    );
    let doc = json!({
        "slot": slot,
        "period": period,
        "entry_count": written.entry_count,
        "size": written.size,
        "written": [named(&written.file), named(INDEX_FILE)],
        "removed": removed,
    });
    Ok((doc, text))
}

/// What a db's backup files are named by, and its backup daemon's units: a
/// db without a registry name is known by its path, and labelled by the
/// path's stem.
pub(crate) fn label(source_db: &str) -> String {
    let name = match source_db.contains(std::path::MAIN_SEPARATOR) {
        true => Path::new(source_db).file_stem().and_then(|s| s.to_str()),
        false => None,
    };
    let safe: String = (name.unwrap_or(source_db).chars())
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            _ => '_',
        })
        .collect();
    match safe.is_empty() {
        true => "db".into(),
        false => safe,
    }
}

/// The slot's file, complete and renamed into place — the half of a run
/// that happens before the index hears of it.
fn write_slot(
    db: &Db,
    dir: &Path,
    source_db: &str,
    slot: u32,
    period: u64,
    now: DateTime<Utc>,
) -> Result<Slot> {
    let file = format!(
        "{}.slot{slot}.{}.jsonl.gz",
        label(source_db),
        now.format("%Y%m%dT%H%M%S%.3fZ")
    );
    let tmp = dir.join(format!("{file}.tmp"));
    let header = BackupHeader::new(db, source_db.to_string(), now)?;
    let out = create_private_file(&tmp).with_context(|| format!("Creating {}", tmp.display()))?;
    let entry_count = write_jsonl_gz(out, db, &header)?;
    std::fs::rename(&tmp, dir.join(&file))?;
    Ok(Slot {
        slot,
        period,
        size: std::fs::metadata(dir.join(&file))?.len(),
        file,
        backup_time: now,
        source_db: source_db.to_string(),
        entry_count,
    })
}

/// The rotation of `old` in DIR made `new`'s, as `db rename` leaves it
/// (da-a0vw0): `DIR/<old>/` moved to `DIR/<new>/` and its index keyed by
/// the new name, so the next run goes on with it and a restore by name finds
/// all of it. One begun flat is keyed again where it is. What it did, to say;
/// `None` where DIR holds no rotation of `old`.
pub(crate) fn rename_rotation(dir: &Path, old: &str, new: &str) -> Result<Option<String>> {
    let (from, to) = match read_index(dir)?.dbs.contains_key(old) {
        true => (dir.to_path_buf(), dir.to_path_buf()),
        false => (dir.join(label(old)), dir.join(label(new))),
    };
    let mut index = read_index(&from)?;
    let Some(rotation) = index.dbs.remove(old).filter(|_| old != new) else {
        return Ok(None);
    };
    if from != to && to.exists() || index.dbs.contains_key(new) {
        return Ok(Some(format!(
            "backups: kept {}, as {} is there already",
            from.display(),
            to.display()
        )));
    }
    index.dbs.insert(new.into(), rotation);
    if from != to {
        std::fs::rename(&from, &to)
            .with_context(|| format!("moving {} to {}", from.display(), to.display()))?;
    }
    write_index(&to, &index)?;
    Ok(Some(match from == to {
        true => format!("backups in {}: {old} → {new}", to.display()),
        false => format!("backups: {} → {}", from.display(), to.display()),
    }))
}

fn read_index(dir: &Path) -> Result<Index> {
    let path = dir.join(INDEX_FILE);
    match std::fs::read(&path) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).with_context(|| format!("Reading {}", path.display()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Index::default()),
        Err(e) => Err(e).with_context(|| format!("Reading {}", path.display())),
    }
}

fn write_index(dir: &Path, index: &Index) -> Result<()> {
    let tmp = dir.join(format!("{INDEX_FILE}.tmp"));
    let mut doc = json!({ "version": INDEX_VERSION, "dbs": index.dbs });
    if let Some(all) = index.all {
        doc["all"] = json!(all);
    }
    std::fs::write(&tmp, serde_json::to_vec_pretty(&doc)?)?;
    std::fs::rename(&tmp, dir.join(INDEX_FILE))?;
    Ok(())
}

/// Every rotation in DIR: the one begun flat at its top, if any, and each
/// subdirectory's, its files named from DIR.
fn read_all(dir: &Path) -> Result<Index> {
    let mut all = read_index(dir)?;
    let subs = match std::fs::read_dir(dir) {
        Ok(it) => it,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(all),
        Err(e) => return Err(e).with_context(|| format!("Reading {}", dir.display())),
    };
    for sub in subs {
        let sub = sub?.path();
        if !sub.join(INDEX_FILE).is_file() {
            continue;
        }
        for (db, mut r) in read_index(&sub)?.dbs {
            let name = sub.file_name().unwrap_or_default();
            for b in &mut r.backups {
                b.file = Path::new(name).join(&b.file).to_string_lossy().into_owned();
            }
            all.dbs.insert(db, r);
        }
    }
    Ok(all)
}

/// When the newest backup of `source_db` in DIR was taken, and its
/// rotation's period, by its index: what a backup daemon of every database
/// reads of each (da-bw7ur).
pub(crate) fn newest(dir: &Path, source_db: &str) -> Option<(DateTime<Utc>, i64)> {
    let index = read_all(dir).ok()?;
    let rotation = index.dbs.get(source_db)?;
    let at = rotation.backups.iter().map(|b| b.backup_time).max()?;
    Some((at, rotation.every_secs))
}

fn list(dir: &Path, now: DateTime<Utc>) -> Result<(serde_json::Value, String)> {
    let index = read_all(dir)?;
    let mut all: Vec<&Slot> = index.dbs.values().flat_map(|r| &r.backups).collect();
    all.sort_by_key(|b| std::cmp::Reverse(b.backup_time));
    let age = |b: &Slot| (now - b.backup_time).num_seconds();
    let backups: Vec<_> = all
        .iter()
        .map(|b| {
            let mut v = json!(b);
            v["age_secs"] = json!(age(b));
            v
        })
        .collect();
    let rotations: BTreeMap<_, _> = index
        .dbs
        .iter()
        .map(|(db, r)| (db, json!({ "every_secs": r.every_secs, "slots": r.slots })))
        .collect();
    let mut text = String::new();
    for b in &all {
        text += &format!(
            "{:>6} ago  {}  slot {}  {} entries  {}  {}\n",
            ago(age(b)),
            b.source_db,
            b.slot,
            b.entry_count,
            bytes(b.size),
            b.file
        );
    }
    if all.is_empty() {
        text = format!("No backups in {}\n", dir.display());
    }
    Ok((json!({ "rotations": rotations, "backups": backups }), text))
}

fn plan_report(every: i64, cap: Option<u32>, span: i64) -> (serde_json::Value, String) {
    let mut held = plan((span / every) as u64, cap);
    held.sort_by_key(|&(_, period)| std::cmp::Reverse(period));
    let age = |p: u64| span - p as i64 * every;
    // Slot 0 always reaches the whole span; the reach is the rotation's.
    let reach = (held.iter())
        .filter(|&&(slot, _)| slot != 0)
        .map(|&(_, p)| age(p))
        .max();
    let doc = json!({
        "every_secs": every,
        "slots": cap,
        "span_secs": span,
        "files": held.len(),
        "reach_secs": reach,
        "backups": held.iter().map(|&(slot, period)| {
            json!({ "slot": slot, "period": period, "age_secs": age(period) })
        }).collect::<Vec<_>>(),
    });
    let mut text = format!(
        "Every {}, {}, after {}: {} files, about {} × one backup's size\n",
        ago(every),
        cap.map_or("no cap".to_string(), |c| format!("{c} slots")),
        ago(span),
        held.len(),
        held.len()
    );
    for &(slot, period) in &held {
        text += &format!("  slot {slot:>2}  {:>6} ago\n", ago(age(period)));
    }
    (doc, text)
}

/// Two days and more in days: a backup's age reaches back months.
pub(crate) fn ago(secs: i64) -> String {
    if secs >= 2 * 86_400 {
        format!("{}d", secs / 86_400)
    } else {
        format_human_duration(secs, DurationPrecision::Minutes)
    }
}

fn bytes(n: u64) -> String {
    match n {
        n if n >= 1 << 20 => format!("{:.1} MiB", n as f64 / (1 << 20) as f64),
        n if n >= 1 << 10 => format!("{:.1} KiB", n as f64 / (1 << 10) as f64),
        n => format!("{n} B"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3600;

    fn t0() -> DateTime<Utc> {
        "2026-01-01T00:00:00Z".parse().unwrap()
    }

    /// One run `p` periods (plus `extra` seconds) after slot 0's time.
    fn run_at(db: &Db, dir: &Path, p: i64, extra: i64, cap: Option<u32>) -> serde_json::Value {
        let now = t0() + chrono::Duration::seconds(p * HOUR + extra);
        rotate(db, dir, "main", every(cap), now).unwrap().0
    }

    fn every(slots: Option<u32>) -> Option<Schedule> {
        Some(Schedule {
            every_secs: HOUR,
            slots,
        })
    }

    fn held(dir: &Path) -> Vec<(u32, u64)> {
        let index = read_index(&dir.join("main")).unwrap();
        index.dbs["main"]
            .backups
            .iter()
            .map(|b| (b.slot, b.period))
            .collect()
    }

    fn setup() -> (Db, tempfile::TempDir) {
        let db = Db::open_in_memory().unwrap();
        db.insert_entry(&datum_core::entry::Entry::new(
            "coffee",
            datum_core::entry::Verb::Point,
            None,
            None,
            None,
            None,
            "d1",
        ))
        .unwrap();
        (db, tempfile::tempdir().unwrap())
    }

    #[test]
    fn periods_land_on_the_hanoi_slots() {
        let first16: Vec<u32> = (1..=16).map(|p| slot_for(p, None)).collect();
        assert_eq!(first16, [1, 2, 1, 3, 1, 2, 1, 4, 1, 2, 1, 3, 1, 2, 1, 5]);
        assert_eq!((slot_for(32, None), slot_for(64, None)), (6, 7));
        assert_eq!(slot_for(0, None), 0);
    }

    #[test]
    fn slot_0_is_never_rewritten_and_the_slots_are_the_plan() {
        let (db, dir) = setup();
        run_at(&db, dir.path(), 0, 0, None);
        let first = read_index(&dir.path().join("main")).unwrap().dbs["main"].backups[0].clone();
        for p in 1..=64 {
            let out = run_at(&db, dir.path(), p, 0, None);
            assert_eq!(out["slot"], slot_for(p as u64, None), "p={p}");
            let index = read_index(&dir.path().join("main")).unwrap();
            let zero = &index.dbs["main"].backups[0];
            assert_eq!((zero.slot, &zero.file), (0, &first.file), "p={p}");
            assert_eq!(zero.backup_time, first.backup_time);
            assert!(dir.path().join("main").join(&first.file).exists());
        }
        assert_eq!(held(dir.path()), plan(64, None));
        // One file per slot, plus the index: nothing left behind.
        assert_eq!(
            std::fs::read_dir(dir.path().join("main")).unwrap().count(),
            plan(64, None).len() + 1
        );
    }

    #[test]
    fn a_run_inside_slot_0s_period_writes_nothing() {
        let (db, dir) = setup();
        run_at(&db, dir.path(), 0, 0, None);
        let out = run_at(&db, dir.path(), 0, HOUR - 1, None);
        assert_eq!(out["slot"], serde_json::Value::Null);
        assert_eq!(held(dir.path()), [(0, 0)]);
    }

    #[test]
    fn a_same_period_rerun_replaces_its_own_slot_only() {
        let (db, dir) = setup();
        for p in 0..=2 {
            run_at(&db, dir.path(), p, 0, None);
        }
        let home = dir.path().join("main");
        let before = read_index(&home).unwrap().dbs.remove("main").unwrap();
        let out = run_at(&db, dir.path(), 2, 1800, None);
        assert_eq!(out["slot"], 2);
        let replaced = format!("main/{}", before.backups[2].file);
        assert_eq!(out["removed"], json!([replaced]));
        let after = read_index(&home).unwrap().dbs.remove("main").unwrap();
        for s in 0..2 {
            assert_eq!(after.backups[s].file, before.backups[s].file);
        }
        assert_ne!(after.backups[2].file, before.backups[2].file);
        assert!(!dir.path().join(replaced).exists());
        assert!(home.join(&after.backups[2].file).exists());
    }

    #[test]
    fn a_capped_rotation_clamps_to_the_top_slot() {
        let (db, dir) = setup();
        for p in 0..=12 {
            run_at(&db, dir.path(), p, 0, Some(2));
        }
        // 1, 3, 5… go to slot 1; every even period to slot 2, the newest (12) kept.
        assert_eq!(held(dir.path()), [(0, 0), (1, 11), (2, 12)]);
        assert_eq!(held(dir.path()), plan(12, Some(2)));
    }

    #[test]
    fn plan_matches_running_every_period() {
        for cap in [None, Some(1), Some(2), Some(3), Some(6)] {
            let mut slots = BTreeMap::from([(0, 0)]);
            for latest in 1..300u64 {
                slots.insert(slot_for(latest, cap), latest);
                let want: Vec<(u32, u64)> = slots.clone().into_iter().collect();
                assert_eq!(plan(latest, cap), want, "latest={latest} cap={cap:?}");
            }
        }
    }

    #[test]
    fn plan_reach_is_the_oldest_rotating_slot_not_slot_0() {
        const DAY: i64 = 86_400;
        // Hourly for 30 days: slot 9 holds period 256, 464h back.
        let (doc, _) = plan_report(HOUR, None, 30 * DAY);
        assert_eq!(doc["reach_secs"], 464 * HOUR);
        // Capped at 8 for a year: five days back.
        let (doc, _) = plan_report(HOUR, Some(8), 365 * DAY);
        assert_eq!(doc["reach_secs"], 5 * DAY);
        // Before the first period ends there is nothing but slot 0.
        let (doc, _) = plan_report(HOUR, None, HOUR - 1);
        assert_eq!(doc["reach_secs"], serde_json::Value::Null);
    }

    #[test]
    fn a_run_killed_before_the_index_leaves_the_old_index_whole() {
        let (db, dir) = setup();
        for p in 0..=2 {
            run_at(&db, dir.path(), p, 0, None);
        }
        let home = dir.path().join("main");
        let before = std::fs::read(home.join(INDEX_FILE)).unwrap();
        // Period 2 again, killed after its file and before the index.
        let now = t0() + chrono::Duration::seconds(2 * HOUR + 1800);
        write_slot(&db, &home, "main", 2, 2, now).unwrap();
        assert_eq!(std::fs::read(home.join(INDEX_FILE)).unwrap(), before);
        for b in &read_index(&dir.path().join("main")).unwrap().dbs["main"].backups {
            let restored = Db::open_in_memory().unwrap();
            let args = crate::args::RestoreArgs {
                path: home.join(&b.file).to_string_lossy().into_owned(),
                force: false,
                dry_run: false,
                merge: true,
                hard: false,
                leave: false,
                chain: false,
                admin_key: None,
                approval: None,
                yes: true,
                if_unchanged: None,
                show: ShowFlag::None,
            };
            super::super::backup_restore::cmd_restore(args, None, &restored).unwrap();
            assert_eq!(
                restored.count_entries().unwrap(),
                b.entry_count,
                "{}",
                b.file
            );
        }
    }

    /// da-o6fvk: two dbs rotated into one DIR at once each keep a rotation of
    /// their own, `DIR/<label>/`, and `--list DIR` reads both.
    #[test]
    fn two_dbs_at_once_each_keep_their_own_index() {
        for _ in 0..20 {
            let dir = tempfile::tempdir().unwrap();
            let start = std::sync::Barrier::new(2);
            std::thread::scope(|s| {
                for name in ["a", "b"] {
                    let (dir, start) = (dir.path(), &start);
                    s.spawn(move || {
                        let (db, _) = setup();
                        start.wait();
                        rotate(&db, dir, name, every(None), t0()).unwrap();
                    });
                }
            });
            for name in ["a", "b"] {
                let index = read_index(&dir.path().join(name)).unwrap();
                assert_eq!(index.dbs.keys().collect::<Vec<_>>(), [name]);
            }
            let (doc, _) = list(dir.path(), t0()).unwrap();
            let mut files: Vec<_> = (doc["backups"].as_array().unwrap().iter())
                .map(|b| b["file"].as_str().unwrap().split('/').next().unwrap())
                .collect();
            files.sort();
            assert_eq!(files, ["a", "b"]);
        }
    }

    /// A db the top index lists, its rotation begun flat before the
    /// subdirectories, is rotated where it is; another db beside it goes to
    /// its own subdirectory.
    #[test]
    fn only_the_db_a_flat_index_lists_stays_flat() {
        let (db, dir) = setup();
        let flat = json!({ "version": 1, "dbs": { "a": {} } });
        std::fs::write(dir.path().join(INDEX_FILE), flat.to_string()).unwrap();
        let run = |name: &str, p: i64| {
            let now = t0() + chrono::Duration::seconds(p * HOUR);
            rotate(&db, dir.path(), name, every(None), now).unwrap().0
        };
        let a = run("a", 0);
        assert_eq!(a["written"][1], INDEX_FILE);
        let b = run("b", 0);
        assert_eq!(b["written"][1], "b/index.json");
        assert_eq!(run("a", 1)["written"][1], INDEX_FILE);
        let top = read_index(dir.path()).unwrap();
        assert_eq!(top.dbs.keys().collect::<Vec<_>>(), ["a"]);
        let (doc, _) = list(dir.path(), t0() + chrono::Duration::seconds(HOUR)).unwrap();
        let mut files: Vec<_> = (doc["backups"].as_array().unwrap().iter())
            .map(|b| b["file"].as_str().unwrap())
            .collect();
        files.sort();
        assert_eq!(files.len(), 3);
        assert_eq!(files[0], a["written"][0]);
        assert!(files[2].starts_with("b/"), "{files:?}");
    }

    /// da-tsna7: a step reads its period and cap from DIR, where the install
    /// recorded them; DIR holding none, it is refused and writes nothing, so a
    /// step by hand starts no rotation. Every database's, at DIR's top, wins
    /// over a database's own.
    #[test]
    fn a_step_takes_the_schedule_dir_records() {
        let (db, dir) = setup();
        let err = rotate(&db, dir.path(), "main", None, t0()).unwrap_err();
        assert!(
            format!("{err:#}").contains("holds no rotation of main"),
            "{err:#}"
        );
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);

        let capped = Schedule {
            every_secs: HOUR,
            slots: Some(2),
        };
        set_schedule(dir.path(), Some("main"), capped).unwrap();
        for p in 0..=12 {
            let now = t0() + chrono::Duration::seconds(p * HOUR);
            rotate(&db, dir.path(), "main", None, now).unwrap();
        }
        assert_eq!(held(dir.path()), plan(12, Some(2)));
        // Recorded again, it keeps the backups held.
        set_schedule(dir.path(), Some("main"), every(None).unwrap()).unwrap();
        let index = read_index(&dir.path().join("main")).unwrap();
        assert_eq!(index.dbs["main"].slots, None);
        assert_eq!(index.dbs["main"].backups.len(), plan(12, Some(2)).len());

        let all = Schedule {
            every_secs: 2 * HOUR,
            slots: None,
        };
        // `--all` by hand into a DIR no `--auto --all` set up is refused whole.
        let mut failed = Vec::new();
        let err = rotate_all(dir.path(), t0(), &mut failed).unwrap_err();
        assert!(
            format!("{err:#}").contains("holds no rotation of every database"),
            "{err:#}"
        );
        set_schedule(dir.path(), None, all).unwrap();
        assert_eq!(read_index(dir.path()).unwrap().all, Some(all));
        rotate_all(dir.path(), t0(), &mut failed).unwrap();
        // The top index lists no database, so none is rotated flat.
        assert!(read_index(dir.path()).unwrap().dbs.is_empty());
    }

    /// A flat rotation whose new name is keyed there already is kept as it
    /// was, both keys on disk (da-a0vw0).
    #[test]
    fn a_flat_rotation_renamed_onto_a_held_name_is_kept_whole() {
        let dir = tempfile::tempdir().unwrap();
        let flat = json!({ "version": 1, "dbs": { "a": {}, "b": {} } });
        std::fs::write(dir.path().join(INDEX_FILE), flat.to_string()).unwrap();
        let said = rename_rotation(dir.path(), "a", "b").unwrap().unwrap();
        assert!(said.starts_with("backups: kept "), "{said}");
        let top = read_index(dir.path()).unwrap();
        assert_eq!(top.dbs.keys().collect::<Vec<_>>(), ["a", "b"]);
        // Without the other, it is keyed again in place.
        rename_rotation(dir.path(), "a", "c").unwrap().unwrap();
        let top = read_index(dir.path()).unwrap();
        assert_eq!(top.dbs.keys().collect::<Vec<_>>(), ["b", "c"]);
    }
}
