//! `datum db import` — bulk-import legacy datum CouchDB backups.
//!
//! Reads a backup JSON file (`{ backupTime, docs: [...] }`), translates each
//! CouchDB doc into a `datum-core::Entry`, and bulk-loads them with
//! `ConflictStrategy::Ignore` so re-importing the same backup is idempotent.
//! Per-row history and per-row conflict detection are skipped on the bulk
//! path; source backups are self-consistent so this is fine, with a manual
//! `datum conflicts list` recommended post-import.

use anyhow::{Context, Result};
use datum_core::{
    db::{ConflictStrategy, Db, InsertOutcome},
    derive_verb,
    entry::{Entry, Verb},
    time::DatumTime,
};
use serde_json::Value as JsonValue;

use crate::{
    args::{DbImportArgs, DbImportArgsExt},
    config::device_id,
};

pub fn import(args: DbImportArgs, db: &Db) -> Result<()> {
    let backup_path = args.resolved_path()?.to_string();
    let dry_run = args.dry_run;

    let content = std::fs::read_to_string(&backup_path)
        .with_context(|| format!("Reading backup file: {}", backup_path))?;
    let backup: serde_json::Value =
        serde_json::from_str(&content).with_context(|| "Parsing backup JSON")?;

    let docs = backup["docs"]
        .as_array()
        .context("Backup JSON missing 'docs' array")?;

    let device = device_id()?;
    let mut skipped = 0usize;
    let mut points = 0usize;
    let mut blocks = 0usize;
    let mut dur_parse_warnings = 0usize;
    let mut entries: Vec<Entry> = Vec::with_capacity(docs.len());
    // Stable provenance timestamp pulled from the backup file itself, not
    // the wall clock at import time. Two imports of the same backup file
    // produce the same _import marker → identical content_hash → dedup.
    // Falls back to "" if the source omits backupTime; this still keeps
    // idempotency (every re-import of that source uses "" too).
    let backup_time = backup["backupTime"].as_str().unwrap_or("").to_string();

    for doc in docs {
        let data = &doc["data"];
        let meta = &doc["meta"];

        let field = match data["field"].as_str() {
            Some(f) => f.to_string(),
            None => {
                skipped += 1;
                continue;
            }
        };

        // ─── state ──────────────────────────────────────────────────
        // `data.state` undefined → state column NULL.
        // `data.state == null` → state column NULL (legacy datum can't
        //   distinguish missing from explicit null on read; collapse them
        //   here so the canonical state goes through derive_verb's
        //   null→true promotion below).
        // anything else → Some(value), preserved verbatim.
        let raw_state = data.get("state");
        let state_in: Option<JsonValue> = match raw_state {
            None => None,
            Some(v) if v.is_null() => None,
            Some(v) => Some(v.clone()),
        };

        let occur_time = parse_datum_time(&data["occurTime"]);
        let occur_time_precision = parse_datum_precision(&data["occurTime"]);

        // ─── dur ────────────────────────────────────────────────────
        // Per da-5ns.6:
        //   legacy `dur` undefined (key missing) → dur=NULL
        //   legacy `dur` null (explicit null)    → dur="PT0S"  (point-for-state)
        //   legacy `dur` "<ISO>"                 → dur="<ISO>" verbatim
        //   legacy `dur` malformed               → dur=NULL, warn, preserve
        //                                          literal in `data` blob
        //   legacy `dur` numeric (rare)          → dur="PT<n>S" (seconds)
        let raw_dur = data.get("dur");
        let dur_in: Option<datum_core::dur::Dur> = match raw_dur {
            None => None,
            Some(v) if v.is_null() => Some(datum_core::dur::Dur::Of(chrono::Duration::zero())),
            Some(JsonValue::String(s)) => {
                if let Some(d) = datum_core::dur::Dur::parse(s) {
                    Some(d)
                } else {
                    eprintln!(
                        "warning: could not parse dur \"{}\" for {}; keeping literal in extra_data",
                        s, field,
                    );
                    dur_parse_warnings += 1;
                    None
                }
            }
            Some(v) => {
                if let Some(secs) = v.as_i64() {
                    Some(datum_core::dur::Dur::Of(chrono::Duration::seconds(secs)))
                } else if let Some(secs) = v.as_f64() {
                    Some(datum_core::dur::Dur::Of(chrono::Duration::seconds(
                        secs as i64,
                    )))
                } else {
                    eprintln!(
                        "warning: could not parse dur {} for {}; keeping literal in extra_data",
                        v, field,
                    );
                    dur_parse_warnings += 1;
                    None
                }
            }
        };

        // ─── verb + state ───────────────────────────────────────────
        // following_state is always None for legacy entries (block-switch /
        // point-switch are daydumb-new shapes — they never appear in a
        // CouchDB dump).
        //
        // We do NOT canonicalize state (the null→true promotion) here.
        // Per the da-5ns.3 contract, `derive_verb_for_storage` owns verb
        // derivation at insert time and runs the canonicalization itself.
        // If we pre-canonicalized — turning a no-state, no-dur legacy doc
        // into `state=true, dur=None` — storage would then re-derive the
        // verb from `(state_set=true, DurKind::None, fs=None)` and write
        // `verb='switch'` instead of the correct `verb='point'` (da-7sr).
        //
        // Run derive_verb purely for input validation (to skip malformed
        // dur / illegal shape combinations mid-bulk rather than letting
        // them fail at insert) and discard its outputs. `verb = Point` is
        // a placeholder that won't trip the switch-style state→true
        // promotion in `derive_verb_for_storage`; storage will compute
        // the real verb from raw `(state_in, dur_in, None)`.
        if let Err(e) = derive_verb(state_in.clone(), dur_in.as_ref(), None) {
            eprintln!("warning: skipping {}: {}", field, e);
            skipped += 1;
            continue;
        }
        let state = state_in.clone();
        let verb = Verb::Point;

        // ─── data blob ──────────────────────────────────────────────
        // Everything in `data` except first-class columns goes into the
        // `data` blob. Note: `dur` is preserved here as user-visible intent
        // (datum-compatible read-back). System provenance lives in
        // `extra_metadata._import` (da-tsw) — strictly disjoint.
        // Doc-root `_rev` and `_attachments` are CouchDB plumbing and never
        // make it into `data` (they live on the doc, not on `data.*`).
        let mut blob = serde_json::Map::new();
        if let Some(obj) = data.as_object() {
            for (k, v) in obj {
                if !matches!(k.as_str(), "field" | "state" | "occurTime") {
                    blob.insert(k.clone(), v.clone());
                }
            }
        }
        let data_blob = if blob.is_empty() {
            None
        } else {
            Some(JsonValue::Object(blob))
        };

        // The "_import" marker distinguishes this row's source. A future
        // datum-v1 importer (bash + text documents) will set
        // source: "datum-v1-shell". Keep this marker shape stable —
        // downstream queries may filter on it (e.g. dedup by `original_id`
        // on re-import).
        let mut import_marker = serde_json::Map::new();
        import_marker.insert(
            "source".to_string(),
            JsonValue::String("datum-v2-couchdb".to_string()),
        );
        import_marker.insert(
            "backup_time".to_string(),
            JsonValue::String(backup_time.clone()),
        );
        if let Some(orig_id) = doc["_id"].as_str() {
            import_marker.insert(
                "original_id".to_string(),
                JsonValue::String(orig_id.to_string()),
            );
        }
        let mut metadata_obj = serde_json::Map::new();
        metadata_obj.insert("_import".to_string(), JsonValue::Object(import_marker));
        let extra_metadata = Some(JsonValue::Object(metadata_obj));

        let created_at = parse_datum_time(&meta["createTime"])
            .map(|t| t.utc)
            .unwrap_or_else(chrono::Utc::now);
        // Prefer source modifyTime; fall back to createTime so imported rows
        // never carry a fresh "now" stamp when the source had a time at all.
        let modified_at = parse_datum_time(&meta["modifyTime"])
            .map(|t| t.utc)
            .unwrap_or(created_at);

        let mut entry = Entry::new(
            field,
            verb,
            state,
            data_blob,
            None, // unit — legacy datum stored unit annotations inside data values
            occur_time,
            dur_in.clone(),
            device.clone(),
        );
        entry.create_time = created_at;
        entry.modify_time = modified_at;
        entry.occur_time_precision = occur_time_precision;
        entry.id_structure = meta
            .get("idStructure")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        entry.field_structure = meta
            .get("fieldStructure")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        // Imported rows weren't produced by a real parse pass; sentinel 0
        // distinguishes them from `CURRENT_PARSER_VERSION` rows so a future
        // re-derive sweep can find and reprocess them.
        entry.parser_version = 0;
        entry.raw_input = String::new();
        entry.alias_chain = None;
        entry.extra_metadata = extra_metadata;
        // Recompute hash after overriding columns that participate in it.
        entry.content_hash = entry.expected_content_hash();

        // Tally based on dur shape (matches user-facing summary semantics:
        // dur=None or dur=zero → point, otherwise → block).
        let is_block = match entry.dur.as_ref() {
            None => false,
            Some(d) if d.is_zero_span() => false,
            Some(datum_core::dur::Dur::Moment) | Some(datum_core::dur::Dur::Unknown) => false,
            Some(_) => true,
        };
        if is_block {
            blocks += 1;
        } else {
            points += 1;
        }

        if dry_run {
            println!("[dry-run] would import: {}", entry.slug);
        }
        entries.push(entry);
    }

    let imported = if dry_run {
        entries.len()
    } else {
        // Bulk-load fast path: single tx, no per-row history rows, no
        // per-row conflict detection. Source backups are self-consistent;
        // run `datum conflicts list` post-import for belt-and-braces.
        // Dedup (`Ignored`) maps to `skipped` to mirror the previous per-row
        // behavior; skipped rows also retract their point/block tally.
        let outcomes = db.insert_entries_bulk(&entries, ConflictStrategy::Ignore)?;
        let mut inserted = 0usize;
        for (entry, outcome) in entries.iter().zip(outcomes.iter()) {
            match outcome {
                InsertOutcome::Inserted { .. } => inserted += 1,
                InsertOutcome::Ignored { .. } => {
                    skipped += 1;
                    match entry.entry_shape {
                        datum_core::entry::EntryShape::Block => {
                            blocks = blocks.saturating_sub(1);
                        }
                        _ => points = points.saturating_sub(1),
                    }
                }
                _ => {}
            }
        }
        inserted
    };

    if dry_run {
        println!(
            "[dry-run] Migration summary: {imported} would import ({points} point, {blocks} block), {skipped} skipped."
        );
    } else {
        println!(
            "Migration complete: {imported} imported ({points} point, {blocks} block), {skipped} skipped."
        );
        if imported > 0 {
            // Bulk-load fast path skipped per-row conflict detection.
            // Source backups are self-consistent so this is normally fine,
            // but give the user a one-liner manual fallback.
            println!("(run `datum conflicts list` to verify nothing slipped through.)");
        }
    }
    if dur_parse_warnings > 0 {
        println!("({dur_parse_warnings} dur values could not be parsed; literals preserved in extra_data.)");
    }
    Ok(())
}

fn parse_datum_time(v: &JsonValue) -> Option<DatumTime> {
    let utc_str = v["utc"].as_str()?;
    let utc: chrono::DateTime<chrono::Utc> = utc_str.parse().ok()?;
    let tz = v["tz"].as_str().unwrap_or("UTC").to_string();
    let offset_minutes = v["o"].as_f64().map(|o| (o * 60.0) as i32).unwrap_or(0);
    Some(DatumTime {
        utc,
        offset_minutes,
        timezone: tz,
    })
}

/// Map a legacy datum `occurTime` block to a single ISO-8601 precision
/// string for the new schema's `occur_time_precision` column.
///
/// Legacy datum stored precision as a `(precision, precisionUnit)` pair on
/// the time block. The v=20 schema collapses that into one column. Per
/// the da-5ns.6 canon table:
///   year → P1Y, month → P1M, day → P1D, week → P1W,
///   hour → PT1H, minute → PT1M, second → PT1S
/// Anything else (including absent, "exact", or unrecognized units) →
/// `None` (treated as exact-second).
fn parse_datum_precision(occur_block: &JsonValue) -> Option<String> {
    let unit = occur_block.get("precisionUnit")?.as_str()?;
    match unit {
        "year" => Some("P1Y".to_string()),
        "month" => Some("P1M".to_string()),
        "day" | "date" => Some("P1D".to_string()),
        "week" => Some("P1W".to_string()),
        "hour" => Some("PT1H".to_string()),
        "minute" => Some("PT1M".to_string()),
        "second" => Some("PT1S".to_string()),
        _ => None,
    }
}
