//! `datum db v2import` — bulk-import legacy datum CouchDB backups.
//!
//! Reads a backup JSON file (`{ backupTime, docs: [...] }`), translates each
//! CouchDB doc into a `datum-core::Entry`, and bulk-loads them with
//! `ConflictStrategy::Ignore` so re-importing the same backup is idempotent.
//! Per-row history and per-row conflict detection are skipped on the bulk
//! path; source backups are self-consistent so this is fine, with a manual
//! `datum conflicts list` recommended post-import.

use std::collections::HashMap;

use anyhow::{Context, Result};
use datum_core::{
    db::{ConflictStrategy, Db, InsertOutcome, IGNORED_STATE_VALUE},
    derive_verb,
    entry::{build_slug, Entry, Verb},
    ids::Hid,
    time::{DatumTime, Zone},
};
use datum_parse::entry_id::DATE_ONLY_PRECISION;
use serde_json::Value as JsonValue;

use crate::{
    args::{DbImportArgs, DbImportArgsExt},
    commands::log_core::normalize_state,
    config::client_id,
    progress::Progress,
};

pub fn import(args: DbImportArgs, db: &Db) -> Result<()> {
    let backup_path = args.resolved_path()?.to_string();
    let dry_run = args.dry_run;
    let skip_log_path = args.skip_log.clone();

    // `-` is the JSON on stdin, as for `db restore -` — the web's file dialog.
    let content = if backup_path == "-" {
        std::io::read_to_string(std::io::stdin()).context("Reading backup from stdin")?
    } else {
        std::fs::read_to_string(&backup_path)
            .with_context(|| format!("Reading backup file: {}", backup_path))?
    };
    let backup: serde_json::Value =
        serde_json::from_str(&content).with_context(|| "Parsing backup JSON")?;

    let docs = backup["docs"]
        .as_array()
        .context("Backup JSON missing 'docs' array")?;

    let client = client_id()?;
    let mut points = 0usize;
    let mut blocks = 0usize;
    let mut dur_parse_warnings = 0usize;
    let mut states_normalized = 0usize;
    let mut entries: Vec<Entry> = Vec::with_capacity(docs.len());
    // Parallel to `entries`: the source `_id` for each entry, so we can
    // attribute `Ignored` (dedup) outcomes back to their original IDs.
    let mut entry_doc_ids: Vec<String> = Vec::with_capacity(docs.len());
    // Parallel to `entries`: the hid lifted from `meta.humanId`, or `None`
    // when the source had no humanId and the entry kept its auto-generated
    // hid. Drives the pre-flight collision check below.
    let mut source_human_ids: Vec<Option<String>> = Vec::with_capacity(docs.len());
    // (reason, id, detail) — id is the source `_id` (or a synthetic
    // <missing-_id at #N> if absent). detail is an optional explanation
    // (the derive_verb error message for `invalid-shape`).
    let mut skipped_records: Vec<(SkipReason, String, Option<String>)> = Vec::new();
    // CouchDB design docs (`_design/…`) are views, not entries: neither
    // imported nor skipped, only counted, so the total still adds up.
    let mut design_docs = 0usize;
    // Stable provenance timestamp pulled from the backup file itself, not
    // the wall clock at import time. Two imports of the same backup file
    // produce the same _import marker → identical content_hash → dedup.
    // Falls back to "" if the source omits backupTime; this still keeps
    // idempotency (every re-import of that source uses "" too).
    let backup_time = backup["backupTime"].as_str().unwrap_or("").to_string();

    let mut reading = Progress::new("v2import: reading", Some(docs.len()));
    for (doc_idx, doc) in docs.iter().enumerate() {
        reading.tick(doc_idx);
        let data = &doc["data"];
        let meta = &doc["meta"];
        let doc_id = doc_id_or_synthetic(doc, doc_idx);
        if doc_id.starts_with("_design/") {
            design_docs += 1;
            continue;
        }

        let field = match data["field"].as_str() {
            Some(f) => f.to_string(),
            None => {
                skipped_records.push((SkipReason::NoField, doc_id, None));
                continue;
            }
        };

        // ─── state ──────────────────────────────────────────────────
        // `data.state` undefined → state column NULL.
        // `data.state == null` → IGNORED, which is what it meant in v2
        //   (da-revva) and what `switch f null` writes today.
        // anything else → Some(value), through `normalize_state` (da-lvyt0):
        //   `{id: x}` → `x`, per element of an array; a structured state
        //   with more keys is kept as it is.
        let raw_state = data.get("state");
        let state_in: Option<JsonValue> = match raw_state {
            None => None,
            Some(v) if v.is_null() => Some(JsonValue::String(IGNORED_STATE_VALUE.to_string())),
            Some(v) => {
                let n = normalize_state(v.clone());
                states_normalized += usize::from(&n != v);
                Some(n)
            }
        };

        let occur_uncertainty = parse_datum_uncertainty(&data["occurTime"]);
        let create_time = DatumTime::from_v2_json(&meta["createTime"]);
        let occur_time = match DatumTime::from_v2_json(&data["occurTime"]) {
            Some(mut t) => {
                t.uncertainty = occur_uncertainty;
                Some(t)
            }
            None => v2_date_only(&data["occurTime"], create_time.as_ref()),
        };

        // ─── dur ────────────────────────────────────────────────────
        // Per da-5ns.6, and da-w94vu for null:
        //   legacy `dur` undefined (key missing) → dur=NULL
        //   legacy `dur` null (explicit null)    → dur=MOMENT  (what
        //                                          `switch … dur=MOMENT` stores)
        //   legacy `dur` "<ISO>"                 → dur="<ISO>" verbatim
        //   legacy `dur` malformed               → dur=NULL, warn, preserve
        //                                          literal in `data` blob
        //   legacy `dur` numeric (rare)          → dur="PT<n>S" (seconds)
        let raw_dur = data.get("dur");
        let dur_in: Option<datum_core::dur::Dur> = match raw_dur {
            None => None,
            Some(v) if v.is_null() => Some(datum_core::dur::Dur::moment()),
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
                    Some(datum_core::dur::Dur::of(chrono::Duration::seconds(secs)))
                } else if let Some(secs) = v.as_f64() {
                    Some(datum_core::dur::Dur::of(chrono::Duration::seconds(
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
            skipped_records.push((SkipReason::InvalidShape, doc_id, Some(e.to_string())));
            continue;
        }
        let state = state_in.clone();
        let verb = Verb::Point;

        // ─── data blob ──────────────────────────────────────────────
        // `data` holds only what has no column (da-w94vu): field, state,
        // occurTime and a `dur` that parsed live in columns, and
        // `lastState` is dropped — v2 required it for CouchDB's views, so
        // it was never something the person asserted. A `dur` that did not
        // parse stays, being the only copy of it. System provenance lives
        // in `extra_metadata._import` (da-tsw) — strictly disjoint.
        // Doc-root `_rev` and `_attachments` are CouchDB plumbing and never
        // make it into `data` (they live on the doc, not on `data.*`).
        let mut blob = serde_json::Map::new();
        if let Some(obj) = data.as_object() {
            for (k, v) in obj {
                let column = match k.as_str() {
                    "field" | "state" | "occurTime" | "lastState" => true,
                    "dur" => dur_in.is_some(),
                    _ => false,
                };
                if !column {
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

        let created_at = create_time.map(|t| t.utc).unwrap_or_else(chrono::Utc::now);
        // Prefer source modifyTime; fall back to createTime so imported rows
        // never carry a fresh "now" stamp when the source had a time at all.
        let modified_at = DatumTime::from_v2_json(&meta["modifyTime"])
            .map(|t| t.utc)
            .unwrap_or(created_at);

        let mut entry = Entry::new(
            field,
            verb,
            state,
            data_blob,
            occur_time,
            dur_in.clone(),
            client.clone(),
        );
        entry.create_time = created_at;
        entry.modify_time = modified_at;
        // A timeless row slugs on its create time, which is the source's,
        // not the clock `Entry::new` read (da-dvyga).
        entry.slug = build_slug(&entry.field, entry.occur_time.as_ref(), &entry.create_time);
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
        // Legacy import: no argv to record. The §5 #11 nullable convention
        // means `None` is the canonical "no raw_input" shape (was
        // empty-string pre-2026-06-02).
        entry.raw_input = None;
        entry.alias_chain = None;
        entry.extra_metadata = extra_metadata;
        // Preserve the source doc's `meta.humanId` as our `hid` so the same
        // doc is addressable by the same handle in both systems during a
        // parallel-run migration. Treat absent / empty as "no source hid"
        // and keep the freshly-generated hid from `Entry::new`. Collision
        // handling is deferred to a pre-flight check below.
        let source_human_id = meta
            .get("humanId")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        if let Some(ref h) = source_human_id {
            entry.hid = Hid::from(h.as_str());
        }
        // Recompute hash after overriding columns that participate in it.
        // (hid is not part of content_hash; recompute remains correct.)
        entry.content_hash = entry.expected_content_hash();

        // Tally based on dur shape (matches user-facing summary semantics:
        // dur=None or dur=zero → point, otherwise → block).
        let is_block = match entry.dur.as_ref() {
            None => false,
            Some(d) if d.is_zero_span() => false,
            Some(d) if d.is_moment() || d.is_unknown() => false,
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
        entry_doc_ids.push(doc_id);
        source_human_ids.push(source_human_id);
    }
    reading.finish(docs.len());

    // Pre-flight: detect hid collisions for entries that lifted their hid
    // from `meta.humanId`. Two flavors:
    //   - In-batch: two source docs declare the same humanId but produce
    //     distinct content. (Same content with same humanId is a benign
    //     duplicate that content_hash dedup will absorb at insert time.)
    //   - With existing DB: an already-stored entry uses this hid for a
    //     different content. Same content → re-import, handled by
    //     content_hash dedup on the bulk path.
    // Either case errors before any write so the existing row stays intact.
    let mut seen_in_batch: HashMap<&str, (&str, &str)> = HashMap::new();
    for ((entry, doc_id), source_hid) in entries
        .iter()
        .zip(entry_doc_ids.iter())
        .zip(source_human_ids.iter())
    {
        let Some(incoming_hid) = source_hid.as_deref() else {
            continue;
        };
        if let Some((prior_doc, prior_hash)) =
            seen_in_batch.insert(incoming_hid, (doc_id.as_str(), entry.content_hash.as_str()))
        {
            if prior_hash != entry.content_hash {
                anyhow::bail!(
                    "hid collision in backup: source docs '{}' and '{}' both declare meta.humanId='{}' with different content; refusing to import (fix the source to deduplicate)",
                    prior_doc,
                    doc_id,
                    incoming_hid,
                );
            }
        }
        match db.get_entry(incoming_hid) {
            Ok(existing) => {
                if existing.content_hash != entry.content_hash {
                    anyhow::bail!(
                        "hid collision: source doc '{}' preserves meta.humanId='{}' but an existing entry (slug={}) already uses that hid for different content; refusing to overwrite (remove or re-key the existing row, then re-run import)",
                        doc_id,
                        incoming_hid,
                        existing.slug,
                    );
                }
                // Same content → re-import; content_hash dedup will skip it.
            }
            Err(datum_core::error::DatumError::NotFound(_)) => { /* free */ }
            Err(e) => return Err(e.into()),
        }
    }

    let imported = if dry_run {
        entries.len()
    } else {
        // Bulk-load fast path: single tx, no per-row history rows, no
        // per-row conflict detection. Source backups are self-consistent;
        // run `datum conflicts list` post-import for belt-and-braces.
        // Dedup (`Ignored`) maps to `skipped` to mirror the previous per-row
        // behavior; skipped rows also retract their point/block tally.
        let mark = datum_core::last_docs::history_mark(db)?;
        let mut inserting = Progress::new("v2import: inserting", Some(entries.len()));
        let outcomes = db.insert_entries_bulk(&entries, ConflictStrategy::Ignore, &mut |n| {
            inserting.tick(n)
        })?;
        inserting.finish(entries.len());
        let mut inserted = 0usize;
        let mut inserted_hids = Vec::new();
        for ((entry, outcome), doc_id) in entries
            .iter()
            .zip(outcomes.iter())
            .zip(entry_doc_ids.iter())
        {
            match outcome {
                InsertOutcome::Inserted { hid, .. } => {
                    inserted += 1;
                    inserted_hids.push(hid.clone());
                }
                InsertOutcome::Ignored { slug } => {
                    // Ignored is a live row with this content, or a row holding
                    // its slug with other content, which is no duplicate
                    // (da-dvyga). A tombstone holds its slug too.
                    let live = match db.get_entry(slug.as_str()) {
                        Ok(e) => Some(e),
                        Err(datum_core::error::DatumError::NotFound(_)) => None,
                        Err(e) => return Err(e.into()),
                    };
                    if live
                        .as_ref()
                        .is_some_and(|h| h.content_hash == entry.content_hash)
                    {
                        skipped_records.push((SkipReason::Duplicate, doc_id.clone(), None));
                    } else {
                        let source_id = |h: Entry| {
                            h.extra_metadata
                                .as_ref()
                                .and_then(|m| m["_import"]["original_id"].as_str())
                                .map_or(h.hid.to_string(), str::to_string)
                        };
                        let held = match live {
                            Some(h) => format!("{} with other content", source_id(h)),
                            None => match db.get_entry_including_deleted(slug.as_str()) {
                                Ok(h) => format!("deleted {}", source_id(h)),
                                Err(datum_core::error::DatumError::NotFound(_)) => {
                                    "a row at its field and time".to_string()
                                }
                                Err(e) => return Err(e.into()),
                            },
                        };
                        skipped_records.push((
                            SkipReason::SlugCollision,
                            doc_id.clone(),
                            Some(format!("slug {slug} held by {held}")),
                        ));
                    }
                    match entry.shape() {
                        datum_core::entry::EntryShape::Block => {
                            blocks = blocks.saturating_sub(1);
                        }
                        _ => points = points.saturating_sub(1),
                    }
                }
                _ => {}
            }
        }
        write_track_specs(db, &entries)?;
        // The whole import is `_LAST` and one action, so one `undo` reverts it
        // (da-yyjob, da-z0zh3): the specs by their history, the bulk rows,
        // which have none, by hid.
        datum_core::last_docs::set_last_to_batch(db, mark, &inserted_hids)?;
        datum_core::undo_log::add_untracked(db.conn(), &inserted_hids)?;
        inserted
    };

    let skipped = skipped_records.len();
    let breakdown = skip_breakdown(&skipped_records);
    if dry_run {
        println!(
            "[dry-run] Migration summary: {imported} would import ({points} point, {blocks} block), {skipped} skipped{breakdown}."
        );
    } else {
        println!(
            "Migration complete: {imported} imported ({points} point, {blocks} block), {skipped} skipped{breakdown}."
        );
    }
    for (reason, id, detail) in &skipped_records {
        match detail {
            Some(d) => println!("  {:<13}  {id}: {d}", reason.as_str()),
            None => println!("  {:<13}  {id}", reason.as_str()),
        }
    }
    if design_docs > 0 {
        let s = if design_docs == 1 { "" } else { "s" };
        println!("{design_docs} CouchDB design doc{s} (_design/…) ignored: views, not entries.");
    }
    if !dry_run && imported > 0 {
        // Bulk-load fast path skipped per-row conflict detection.
        // Source backups are self-consistent so this is normally fine,
        // but give the user a one-liner manual fallback.
        println!("(run `datum conflicts list` to verify nothing slipped through.)");
    }
    if states_normalized > 0 {
        println!("states normalized: {states_normalized} ({{id: x}} collapsed to x)");
    }
    if dur_parse_warnings > 0 {
        println!("({dur_parse_warnings} dur values could not be parsed; literals preserved in extra_data.)");
    }
    if let Some(path) = skip_log_path.as_deref() {
        write_skip_log(path, &backup_path, &skipped_records)
            .with_context(|| format!("writing skip log: {path}"))?;
        if skipped > 0 {
            println!("(skipped IDs written to {path})");
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkipReason {
    /// Source doc had no `data.field` — can't be imported.
    NoField,
    /// `derive_verb` rejected the (state, dur) shape.
    InvalidShape,
    /// Already present in the target DB (content_hash dedup).
    Duplicate,
    /// Its slug is held by a row with other content.
    SlugCollision,
}

impl SkipReason {
    fn as_str(self) -> &'static str {
        match self {
            SkipReason::NoField => "no-field",
            SkipReason::InvalidShape => "invalid-shape",
            SkipReason::Duplicate => "duplicate",
            SkipReason::SlugCollision => "slug-collision",
        }
    }
}

fn doc_id_or_synthetic(doc: &JsonValue, idx: usize) -> String {
    doc["_id"]
        .as_str()
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("<missing-_id at #{idx}>"))
}

fn skip_breakdown(records: &[(SkipReason, String, Option<String>)]) -> String {
    if records.is_empty() {
        return String::new();
    }
    let (mut nf, mut inv, mut dup, mut col) = (0usize, 0usize, 0usize, 0usize);
    for (r, _, _) in records {
        match r {
            SkipReason::NoField => nf += 1,
            SkipReason::InvalidShape => inv += 1,
            SkipReason::Duplicate => dup += 1,
            SkipReason::SlugCollision => col += 1,
        }
    }
    let mut parts = Vec::new();
    if dup > 0 {
        parts.push(format!("{dup} duplicate"));
    }
    if col > 0 {
        parts.push(format!("{col} slug-collision"));
    }
    if inv > 0 {
        parts.push(format!("{inv} invalid-shape"));
    }
    if nf > 0 {
        parts.push(format!("{nf} no-field"));
    }
    format!(" ({})", parts.join(", "))
}

fn write_skip_log(
    path: &str,
    backup_path: &str,
    records: &[(SkipReason, String, Option<String>)],
) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut f = std::fs::File::create(path)?;
    writeln!(f, "# datum db v2import skip log")?;
    writeln!(f, "# backup: {backup_path}")?;
    writeln!(f, "# generated: {}", chrono::Utc::now().to_rfc3339())?;
    writeln!(f, "# format: <reason>\\t<id>\\t<detail>")?;
    for (reason, id, detail) in records {
        let detail = detail.as_deref().unwrap_or("");
        writeln!(f, "{}\t{}\t{}", reason.as_str(), id, detail)?;
    }
    Ok(())
}

/// v2 writes a date-only time (`-D`, or `-d`/`-y` with no time) as
/// `{utc: "YYYY-MM-DD"}` with no zone. It imports as `occur -D` writes one:
/// local midnight with [`DATE_ONLY_PRECISION`], in the zone of the doc's
/// createTime (UTC without one), so the calendar date stays v2's (da-dvyga).
fn v2_date_only(occur: &JsonValue, create: Option<&DatumTime>) -> Option<DatumTime> {
    let date: chrono::NaiveDate = occur.get("utc")?.as_str()?.parse().ok()?;
    // A zone name that does not parse leaves the offset, still known.
    let (zone, timezone) = match create {
        Some(t) => match Zone::of(t) {
            Ok(z) => (z, t.timezone.clone()),
            Err(_) => (Zone::Fixed(*t.local_time().offset()), None),
        },
        None => (Zone::Fixed(chrono::FixedOffset::east_opt(0)?), None),
    };
    let utc = zone.resolve(date.and_hms_opt(0, 0, 0)?).for_boundary();
    Some(DatumTime {
        utc,
        offset_minutes: zone.offset_at(utc).local_minus_utc() / 60,
        timezone,
        uncertainty: Some(DATE_ONLY_PRECISION.to_string()),
    })
}

/// Map a legacy datum `occurTime` block to a single ISO-8601 uncertainty
/// string for `DatumTime::uncertainty` (column `occur_time_uncertainty`).
///
/// Legacy datum stored this as a `(precision, precisionUnit)` pair on the
/// time block. The v=20 schema collapses that into one column. Per the
/// da-5ns.6 canon table:
///   year → P1Y, month → P1M, day → P1D, week → P1W,
///   hour → PT1H, minute → PT1M, second → PT1S
/// Anything else (including absent, "exact", or unrecognized units) →
/// `None`, i.e. unknown — the legacy backup did not tell us.
fn parse_datum_uncertainty(occur_block: &JsonValue) -> Option<String> {
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

/// An imported database is whole under the same rule a typed one is: every
/// field carries a spec with the verb its imported entries imply. The spec
/// alone records that the field exists (da-gvcen); da-c8mh's TRACK row, a
/// millisecond ahead of the field's first entry, was an event that never
/// happened and is no longer written (da-r6ofr).
fn write_track_specs(db: &Db, entries: &[datum_core::entry::Entry]) -> Result<()> {
    let fields: std::collections::BTreeSet<&str> =
        entries.iter().map(|e| e.field.as_str()).collect();
    for field in fields {
        if db.load_spec(field)?.is_some() {
            continue;
        }
        if let Some(verb) = db.verb_from_field_log(field)? {
            crate::commands::helpers::save_track_spec(db, field, verb)?;
        }
    }
    Ok(())
}
