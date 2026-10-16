//! `datum db backup --to DIR --every D [--slots N]` — a Tower of Hanoi
//! rotation of backups kept in one directory (da-d5cmj).
//!
//! Slot 0 is the first backup ever taken in DIR for a db, and nothing
//! overwrites it. Every later backup is keyed by its period, `p = floor((now
//! − slot 0's time) / every)`, and lands in slot `1 + trailing_zeros(p)`,
//! clamped to `--slots`: a re-run in the same period replaces its own slot,
//! and a period nobody ran is simply absent. Each slot is one gzipped JSONL
//! file — the ordinary backup format, so `db restore` reads it — and
//! `index.json` says what is there. The index is written after the slot
//! file is whole and the slot's previous file removed only after that, so a
//! run killed anywhere leaves an index naming whole files.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use datum_core::db::Db;
use datum_parse::time::{format_human_duration, DurationPrecision};
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::backup_restore::{create_private_file, derive_source_db, write_jsonl_gz, BackupHeader};
use crate::args::{BackupRotationArgs, RotationAction, ShowFlag};

const INDEX_FILE: &str = "index.json";
const INDEX_VERSION: u32 = 1;

/// `index.json`: per db, the rotation's settings and the backups it holds.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    dbs: BTreeMap<String, Rotation>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Rotation {
    every_secs: i64,
    /// The cap, slot 0 not counted; `None` grows with time.
    slots: Option<u32>,
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
    let (doc, text) = match &args.action {
        RotationAction::To(dir) => {
            let source_db = derive_source_db(cli_db);
            rotate(db, Path::new(dir), &source_db, args.every, args.slots, now)?
        }
        RotationAction::List(dir) => list(Path::new(dir), now)?,
        RotationAction::Plan { span } => plan_report(args.every, args.slots, *span),
    };
    match args.show {
        ShowFlag::None => {}
        ShowFlag::Json => println!("{doc}"),
        _ => print!("{text}"),
    }
    Ok(())
}

fn rotate(
    db: &Db,
    dir: &Path,
    source_db: &str,
    every: i64,
    cap: Option<u32>,
    now: DateTime<Utc>,
) -> Result<(serde_json::Value, String)> {
    std::fs::create_dir_all(dir).with_context(|| format!("Creating {}", dir.display()))?;
    let mut index = read_index(dir)?;
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
            o.file
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
        "written": [written.file, INDEX_FILE],
        "removed": removed,
    });
    Ok((doc, text))
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
    // A db without a registry name is known by its path; its file, by the
    // path's stem.
    let name = match source_db.contains(std::path::MAIN_SEPARATOR) {
        true => Path::new(source_db).file_stem().and_then(|s| s.to_str()),
        false => None,
    };
    let mut safe: String = (name.unwrap_or(source_db).chars())
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
            _ => '_',
        })
        .collect();
    if safe.is_empty() {
        safe = "db".into();
    }
    let file = format!(
        "{safe}.slot{slot}.{}.jsonl.gz",
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
    let doc = json!({ "version": INDEX_VERSION, "dbs": index.dbs });
    std::fs::write(&tmp, serde_json::to_vec_pretty(&doc)?)?;
    std::fs::rename(&tmp, dir.join(INDEX_FILE))?;
    Ok(())
}

fn list(dir: &Path, now: DateTime<Utc>) -> Result<(serde_json::Value, String)> {
    let index = read_index(dir)?;
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
fn ago(secs: i64) -> String {
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
        rotate(db, dir, "main", HOUR, cap, now).unwrap().0
    }

    fn held(dir: &Path) -> Vec<(u32, u64)> {
        let index = read_index(dir).unwrap();
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
        let first = read_index(dir.path()).unwrap().dbs["main"].backups[0].clone();
        for p in 1..=64 {
            let out = run_at(&db, dir.path(), p, 0, None);
            assert_eq!(out["slot"], slot_for(p as u64, None), "p={p}");
            let index = read_index(dir.path()).unwrap();
            let zero = &index.dbs["main"].backups[0];
            assert_eq!((zero.slot, &zero.file), (0, &first.file), "p={p}");
            assert_eq!(zero.backup_time, first.backup_time);
            assert!(dir.path().join(&first.file).exists());
        }
        assert_eq!(held(dir.path()), plan(64, None));
        // One file per slot, plus the index: nothing left behind.
        assert_eq!(
            std::fs::read_dir(dir.path()).unwrap().count(),
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
        let before = read_index(dir.path()).unwrap().dbs.remove("main").unwrap();
        let out = run_at(&db, dir.path(), 2, 1800, None);
        assert_eq!(out["slot"], 2);
        assert_eq!(out["removed"], json!([before.backups[2].file]));
        let after = read_index(dir.path()).unwrap().dbs.remove("main").unwrap();
        for s in 0..2 {
            assert_eq!(after.backups[s].file, before.backups[s].file);
        }
        assert_ne!(after.backups[2].file, before.backups[2].file);
        assert!(!dir.path().join(&before.backups[2].file).exists());
        assert!(dir.path().join(&after.backups[2].file).exists());
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
        let before = std::fs::read(dir.path().join(INDEX_FILE)).unwrap();
        // Period 2 again, killed after its file and before the index.
        let now = t0() + chrono::Duration::seconds(2 * HOUR + 1800);
        write_slot(&db, dir.path(), "main", 2, 2, now).unwrap();
        assert_eq!(std::fs::read(dir.path().join(INDEX_FILE)).unwrap(), before);
        for b in &read_index(dir.path()).unwrap().dbs["main"].backups {
            let restored = Db::open_in_memory().unwrap();
            let args = crate::args::RestoreArgs {
                path: dir.path().join(&b.file).to_string_lossy().into_owned(),
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
}
