//! Changeset pack/apply against a local `datum_core::db::Db`.
//!
//! This is the bridge between the wire protocol (opaque bytes in
//! [`crate::crypto::changeset::Changeset`]) and the on-device SQLite
//! entries table. Gated by the `db` feature because it depends on
//! `datum-core` and rusqlite — WASM builds that supply their own storage
//! layer should skip this module entirely and reimplement pack/apply
//! against their schema.
//!
//! # Watermark model
//!
//! - `since_modify_ms` in [`pack_changeset`] is a local watermark: the largest
//!   `modify_time_ms` (epoch-ms) already included in a prior pack. Passing
//!   `0` produces a full dump of every live and tombstoned entry. The push
//!   path uses [`pack_changeset_since_ms`] to advance the watermark to the max
//!   metric of exactly the rows it packed. This is an epoch-ms value, NOT a
//!   seq: "seq" in this crate means either the per-device push counter bound
//!   into AAD or the server-assigned global row id used as pull watermark.
//! - [`crate::sync_state::advance_push_mark`] /
//!   [`crate::sync_state::advance_pull_seq`] record the latest seq the
//!   relay accepted on a push and the latest server-assigned global seq
//!   consumed on a pull.

use datum_core::db::Db;
use datum_core::entry::Entry;
use datum_core::error::{DatumError, Result};
use datum_core::history::{self, ChangeKind};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::crypto::changeset::Changeset;
use datum_sync_protocol::crypto::constants::CHANGESET_MAGIC_V1;

/// Internal v1 payload: a flat list of entries (live + tombstoned).
/// Tombstones carry `deleted_at = Some(..)`; peers apply them as soft
/// deletes.
#[derive(Serialize, Deserialize)]
struct ChangesetPayloadV1 {
    entries: Vec<Entry>,
}

/// Pack every entry (including tombstones) whose `modified_at` epoch-ms is
/// strictly greater than `since_modify_ms`. Passing `0` produces a full dump.
///
/// The returned changeset is self-contained plaintext bytes — the caller
/// is expected to pipe it through
/// [`crate::crypto::changeset::encrypt_changeset`] before it leaves the
/// device.
pub fn pack_changeset(db: &Db, since_modify_ms: u64) -> Result<Changeset> {
    Ok(pack_changeset_since_ms(db, since_modify_ms)?.0)
}

/// Like [`pack_changeset`] but also returns the largest `modify_time_ms`
/// (epoch-ms metric) among the packed rows — the value the caller advances
/// the push watermark to. Both the row filter and this maximum read the same
/// `modify_time_ms` generated column in a single query, so the watermark
/// exactly matches the packed set (no chrono-vs-SQL drift, no TOCTOU against
/// a concurrent writer) and re-using it as the next `since_modify_ms` cannot
/// skip a row. Returns `None` when nothing was packed.
pub fn pack_changeset_since_ms(db: &Db, since_modify_ms: u64) -> Result<(Changeset, Option<u64>)> {
    let (entries, max_ms) = select_entries_since(db, since_modify_ms)?;
    let payload = ChangesetPayloadV1 { entries };
    let json = serde_json::to_vec(&payload)?;
    let mut bytes = Vec::with_capacity(CHANGESET_MAGIC_V1.len() + json.len());
    bytes.extend_from_slice(CHANGESET_MAGIC_V1);
    bytes.extend_from_slice(&json);
    Ok((Changeset::from_bytes(bytes), max_ms))
}

/// Apply a received changeset to the local DB using last-writer-wins on
/// `modify_time`. Existing rows whose stored `modify_time` is greater than
/// the incoming row's are left untouched; exact ties are broken by a
/// deterministic total order on `(content_hash, device_id)` so every
/// device resolves the same winner (see [`upsert_lww`]).
///
/// `modify_time` is a raw device wall-clock: a device with a fast clock
/// wins every tie and a lagging one can silently lose a later edit. This
/// clock-skew risk is an accepted v1 simplification (no HLC/vector clock)
/// — see `docs/sync-protocol.md` §Merge semantics. The risk is bounded
/// because every non-no-op overwrite is both preserved in `entry_history`
/// (`sync_merge`) and surfaced as a `lww_overwrite` integrity warning
/// (see [`upsert_lww`]).
pub fn apply_changeset(db: &Db, cs: &Changeset) -> Result<()> {
    let payload = parse_payload(cs.as_bytes())?;
    let tx = db.conn().unchecked_transaction()?;
    for entry in payload.entries.iter() {
        upsert_lww(&tx, entry)?;
    }
    tx.commit()?;
    Ok(())
}

/// Validate the locked changeset envelope (4-byte magic + JSON, see
/// `docs/sync-protocol.md`) and return the JSON slice after the magic.
/// Every reader of a packed changeset must come through here so a future
/// magic bump (DSC2) is rejected uniformly instead of being mis-parsed
/// as v1 by a blind 4-byte split (da-pl2o).
fn payload_json(bytes: &[u8]) -> Result<&[u8]> {
    if bytes.len() < CHANGESET_MAGIC_V1.len() {
        return Err(DatumError::Other("changeset too short".to_string()));
    }
    let (magic, rest) = bytes.split_at(CHANGESET_MAGIC_V1.len());
    if magic != CHANGESET_MAGIC_V1 {
        return Err(DatumError::Other(format!(
            "unknown changeset magic: {:02x?}",
            magic
        )));
    }
    Ok(rest)
}

fn parse_payload(bytes: &[u8]) -> Result<ChangesetPayloadV1> {
    Ok(serde_json::from_slice(payload_json(bytes)?)?)
}

/// Decode the entry list from a packed changeset, validating the envelope
/// first. Shared with `sync_once` (conflict snapshot + dry-run counters)
/// so the crate has exactly one envelope parser.
pub(crate) fn changeset_entries(bytes: &[u8]) -> Result<Vec<Entry>> {
    Ok(parse_payload(bytes)?.entries)
}

/// Select every entry whose `modify_time_ms` exceeds `since_ms`, returning
/// the rows and the maximum `modify_time_ms` among them.
///
/// The watermark filter is pushed into SQL against the indexed
/// `modify_time_ms` generated column (da-x3da), so this is a b-tree range
/// seek rather than a full-table scan + Rust-side filter. `since_ms == 0`
/// still selects every live/tombstoned row (a full dump). The max is taken
/// from the same query so it is exactly the largest metric in the packed set.
fn select_entries_since(db: &Db, since_ms: u64) -> Result<(Vec<Entry>, Option<u64>)> {
    // Alias `modify_time_ms` so we can read it by a stable name regardless of
    // whether `SELECT *` surfaces the generated column on this SQLite build.
    let mut stmt = db.conn().prepare(
        "SELECT *, modify_time_ms AS _mtms FROM entries \
         WHERE modify_time_ms > ?1 ORDER BY modify_time_ms",
    )?;
    let rows = stmt.query_map([since_ms as i64], |row| {
        let entry = datum_core::db::row_to_entry_public(row)?;
        let ms: i64 = row.get("_mtms")?;
        Ok((entry, ms))
    })?;
    let mut out = Vec::new();
    let mut max_ms: Option<u64> = None;
    for row in rows {
        let (entry, ms) = row?;
        if ms >= 0 {
            let ms = ms as u64;
            max_ms = Some(max_ms.map_or(ms, |cur| cur.max(ms)));
        }
        out.push(entry);
    }
    Ok((out, max_ms))
}

fn upsert_lww(tx: &rusqlite::Transaction<'_>, entry: &Entry) -> Result<()> {
    // Skip rows we already have as newer (or as the tie winner, below).
    // Also pull the current full-row so we can snapshot it into
    // entry_history when an apply actually changes state — no-op LWW
    // short-circuits do NOT write history.
    let existing: Option<Entry> = tx
        .query_row(
            "SELECT * FROM entries WHERE slug = ?1",
            params![entry.slug],
            datum_core::db::row_to_entry_public,
        )
        .optional()?;
    if let Some(pre) = existing.as_ref() {
        match pre.modify_time.cmp(&entry.modify_time) {
            // Strictly newer local row: incoming loses, plain LWW.
            std::cmp::Ordering::Greater => return Ok(()),
            // Exact tie. Identical content (same content_hash) is a replay
            // of a row we already have — skip without history/warning spam.
            // Different content in the same instant must NOT let each device
            // keep its own row (permanent silent divergence, da-22zm):
            // break the tie with a deterministic total order on
            // (content_hash, device_id), compared lexically, so every
            // device picks the same winner. This tiebreaker is part of the
            // sync contract — all clients must implement it identically
            // (docs/sync-protocol.md §Merge semantics).
            std::cmp::Ordering::Equal => {
                if pre.content_hash == entry.content_hash
                    || (&pre.content_hash, &pre.device_id) > (&entry.content_hash, &entry.device_id)
                {
                    return Ok(());
                }
            }
            // Strictly newer incoming row: overwrite (LWW).
            std::cmp::Ordering::Less => {}
        }
        // Non-no-op: record the pre-merge state as a sync_merge history
        // row (recoverable + auditable), and file a `lww_overwrite`
        // integrity warning so the overwrite is surfaced to the user (C4e)
        // rather than applied silently. The history row is the recoverable
        // source; the warning is the notification keyed off it (da-t96a).
        // Only the existing-row branch is an overwrite — a fresh arrival
        // (the `else` below) replaces nothing local and files no warning.
        history::record(tx, pre, ChangeKind::SyncMerge)?;
        datum_core::integrity::file_lww_overwrite(tx, pre, entry)?;
    } else {
        // Fresh row arriving from a peer — the "pre" state is the incoming
        // row itself (same as a create). Record it as sync_merge so the
        // change_kind reflects how it got here, matching the protocol spec.
        history::record(tx, entry, ChangeKind::SyncMerge)?;
    }

    // da-5ns sub-bead 1: write the new (occur_time, state, dur,
    // following_state, verb) shape. Sub-bead 3 wires verb-derivation
    // through this path; for now the stored verb is whatever the peer
    // sent.
    let occur_str = entry
        .occur_time
        .as_ref()
        .map(|t| t.utc.to_rfc3339())
        .unwrap_or_else(|| entry.create_time.to_rfc3339());
    let occur_offset = entry.occur_time.as_ref().map(|t| t.offset_minutes);
    let occur_tz = entry
        .occur_time
        .as_ref()
        .map(|t| t.timezone.as_str().to_string());
    let verb_str = entry.verb.to_string();

    tx.execute(
        r#"INSERT INTO entries (
            hid, slug, content_hash, field,
            verb, state, dur, following_state, data,
            occur_time, occur_time_offset, occur_time_tz, occur_time_precision,
            dur_precision,
            lat, lon, location,
            create_time, modify_time, device_id,
            deleted_at, id_structure, field_structure,
            raw_input, alias_chain, parser_version,
            extra_metadata
        ) VALUES (
            ?1, ?2, ?3, ?4,
            ?5, ?6, ?7, ?8, ?9,
            ?10, ?11, ?12, ?13,
            ?14,
            ?15, ?16, ?17,
            ?18, ?19, ?20,
            ?21, ?22, ?23,
            ?24, ?25, ?26,
            ?27
        )
        ON CONFLICT(slug) DO UPDATE SET
            -- Merge keys on slug (the durable cross-device identity). On
            -- conflict the row takes the LWW winner's hid, so hid is NOT
            -- stable across a cross-device merge — slug is. Code must not
            -- assume hid permanence across sync.
            hid             = excluded.hid,
            content_hash    = excluded.content_hash,
            field           = excluded.field,
            verb            = excluded.verb,
            state           = excluded.state,
            dur             = excluded.dur,
            following_state = excluded.following_state,
            data            = excluded.data,
            occur_time      = excluded.occur_time,
            occur_time_offset = excluded.occur_time_offset,
            occur_time_tz   = excluded.occur_time_tz,
            occur_time_precision = excluded.occur_time_precision,
            dur_precision   = excluded.dur_precision,
            lat             = excluded.lat,
            lon             = excluded.lon,
            location        = excluded.location,
            create_time     = excluded.create_time,
            modify_time     = excluded.modify_time,
            device_id       = excluded.device_id,
            deleted_at      = excluded.deleted_at,
            id_structure    = excluded.id_structure,
            field_structure = excluded.field_structure,
            raw_input       = excluded.raw_input,
            alias_chain     = excluded.alias_chain,
            parser_version  = excluded.parser_version,
            extra_metadata  = excluded.extra_metadata"#,
        params![
            entry.hid,
            entry.slug,
            entry.content_hash,
            entry.field,
            verb_str,
            entry.state.as_ref().map(|v| v.to_string()),
            entry
                .dur
                .as_ref()
                .map(datum_core::dur::Dur::to_storage_string),
            entry.following_state.as_ref().map(|v| v.to_string()),
            entry.data.as_ref().map(|v| v.to_string()),
            occur_str,
            occur_offset,
            occur_tz,
            entry.occur_time_precision,
            entry.dur_precision,
            entry.lat,
            entry.lon,
            entry.location.as_deref(),
            entry.create_time.to_rfc3339(),
            entry.modify_time.to_rfc3339(),
            entry.device_id,
            entry.deleted_at.map(|t| t.to_rfc3339()),
            entry.id_structure.as_deref(),
            entry.field_structure.as_deref(),
            entry.raw_input,
            entry.alias_chain.as_ref().map(|v| v.to_string()),
            entry.parser_version,
            entry.extra_metadata.as_ref().map(|v| v.to_string()),
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone, Utc};
    use datum_core::entry::{Entry, Verb};
    use datum_core::time::DatumTime;

    fn fresh_db() -> Db {
        Db::open_in_memory().unwrap()
    }

    fn make_entry(field: &str, offset_secs: i64) -> Entry {
        let base = Utc.with_ymd_and_hms(2026, 4, 21, 12, 0, 0).unwrap();
        let at = base + Duration::seconds(offset_secs);
        let t = DatumTime {
            utc: at,
            offset_minutes: 0,
            timezone: "UTC".to_string(),
        };
        let mut e = Entry::new(
            field,
            Verb::Point,
            Some(serde_json::json!(1)),
            None,
            Some(t),
            None,
            "dev-1",
        );
        e.create_time = at;
        e.modify_time = at;
        e
    }

    #[test]
    fn pack_apply_round_trip() {
        let db1 = fresh_db();
        let db2 = fresh_db();
        db1.insert_entry(&make_entry("coffee", 0)).unwrap();
        db1.insert_entry(&make_entry("mood", 60)).unwrap();

        let cs = pack_changeset(&db1, 0).unwrap();
        apply_changeset(&db2, &cs).unwrap();

        let count: i64 = db2
            .conn()
            .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);

        let cs2 = pack_changeset(&db2, 0).unwrap();
        let p1 = parse_payload(cs.as_bytes()).unwrap();
        let p2 = parse_payload(cs2.as_bytes()).unwrap();
        assert_eq!(p1.entries.len(), p2.entries.len());
    }

    #[test]
    fn pack_since_modify_ms_returns_only_new_rows() {
        let db = fresh_db();
        let e1 = make_entry("coffee", 0);
        let e2 = make_entry("mood", 120);

        db.insert_entry(&e1).unwrap();
        let watermark_ms = e1.modify_time.timestamp_millis() as u64;

        db.insert_entry(&e2).unwrap();
        let cs = pack_changeset(&db, watermark_ms).unwrap();
        let payload = parse_payload(cs.as_bytes()).unwrap();
        assert_eq!(payload.entries.len(), 1);
        assert_eq!(payload.entries[0].field, "mood");
    }

    /// The watermark filter must hit the `entries_modify_time_ms` index, not
    /// fall back to a full scan — that is the whole point of da-x3da.
    #[test]
    fn pack_filter_uses_modify_time_ms_index() {
        let db = fresh_db();
        let plan: String = db
            .conn()
            .query_row(
                "EXPLAIN QUERY PLAN SELECT *, modify_time_ms AS _mtms FROM entries \
                 WHERE modify_time_ms > ?1 ORDER BY modify_time_ms",
                [0_i64],
                |row| row.get(3),
            )
            .unwrap();
        assert!(
            plan.contains("entries_modify_time_ms"),
            "expected index seek, planner chose: {plan}"
        );
    }

    /// Re-using the returned max as the next watermark packs each row exactly
    /// once: a second pack from that watermark, with no new writes, is empty.
    #[test]
    fn watermark_from_max_packs_each_row_once() {
        let db = fresh_db();
        db.insert_entry(&make_entry("coffee", 0)).unwrap();
        db.insert_entry(&make_entry("mood", 120)).unwrap();

        let (cs, max_ms) = pack_changeset_since_ms(&db, 0).unwrap();
        assert_eq!(parse_payload(cs.as_bytes()).unwrap().entries.len(), 2);
        let watermark = max_ms.expect("packed rows yield a watermark");

        let (cs2, max2) = pack_changeset_since_ms(&db, watermark).unwrap();
        assert_eq!(
            parse_payload(cs2.as_bytes()).unwrap().entries.len(),
            0,
            "rows at or below the advanced watermark must not re-pack"
        );
        assert_eq!(max2, None);
    }

    /// Every envelope reader must reject an unknown magic instead of
    /// blind-splitting at 4 bytes and mis-parsing a future version as v1
    /// (da-pl2o: a DSC2 blob must error everywhere, uniformly).
    #[test]
    fn unknown_magic_is_rejected_not_misparsed() {
        let db = fresh_db();
        db.insert_entry(&make_entry("coffee", 0)).unwrap();
        let mut bytes = pack_changeset(&db, 0).unwrap().as_bytes().to_vec();
        bytes[..4].copy_from_slice(b"DSC2");

        let err = changeset_entries(&bytes).unwrap_err();
        assert!(err.to_string().contains("unknown changeset magic"));
        assert!(changeset_entries(b"DS").is_err(), "short blob must error");
    }

    #[test]
    fn apply_changeset_lww_skips_older_incoming() {
        let db_a = fresh_db();
        let db_b = fresh_db();

        let mut older = make_entry("coffee", 0);
        let mut newer = make_entry("coffee", 0);
        newer.modify_time = older.modify_time + Duration::seconds(60);
        // Distinguish older vs newer by real content — the storage layer
        // canonicalizes content_hash (da-r569), so a synthetic label on
        // `entry.content_hash` no longer survives the insert.
        newer.state = Some(serde_json::json!("newer"));
        older.state = Some(serde_json::json!("older"));

        db_a.insert_entry(&newer).unwrap();

        let older_cs = {
            let tmp = fresh_db();
            tmp.insert_entry(&older).unwrap();
            pack_changeset(&tmp, 0).unwrap()
        };
        let newer_cs = pack_changeset(&db_a, 0).unwrap();

        apply_changeset(&db_b, &older_cs).unwrap();
        apply_changeset(&db_b, &newer_cs).unwrap();
        apply_changeset(&db_b, &older_cs).unwrap();

        let landed = db_b.get_entry(&newer.slug).unwrap();
        assert_eq!(landed.state, Some(serde_json::json!("newer")));
    }

    /// Equal `modify_time` with different content must converge to the
    /// same winner on every device (deterministic tiebreaker on
    /// `(content_hash, device_id)`), not let each side keep its own row
    /// (da-22zm: permanent silent divergence).
    #[test]
    fn equal_modify_time_tie_converges_deterministically() {
        let db_a = fresh_db();
        let db_b = fresh_db();

        // Same slug, same modify_time, different content on each device.
        let mut on_a = make_entry("coffee", 0);
        let mut on_b = make_entry("coffee", 0);
        on_a.state = Some(serde_json::json!("from-a"));
        on_b.state = Some(serde_json::json!("from-b"));
        on_b.device_id = "dev-2".to_string();
        assert_eq!(on_a.slug, on_b.slug);
        assert_eq!(on_a.modify_time, on_b.modify_time);

        db_a.insert_entry(&on_a).unwrap();
        db_b.insert_entry(&on_b).unwrap();

        // Full cross-sync: each device applies the other's changeset.
        let cs_a = pack_changeset(&db_a, 0).unwrap();
        let cs_b = pack_changeset(&db_b, 0).unwrap();
        apply_changeset(&db_a, &cs_b).unwrap();
        apply_changeset(&db_b, &cs_a).unwrap();

        let landed_a = db_a.get_entry(&on_a.slug).unwrap();
        let landed_b = db_b.get_entry(&on_b.slug).unwrap();
        assert_eq!(
            landed_a.content_hash, landed_b.content_hash,
            "equal-stamp tie must resolve to the same winner on both devices"
        );
        assert_eq!(landed_a.state, landed_b.state);
        assert_eq!(landed_a.device_id, landed_b.device_id);
    }

    /// An equal-`modify_time` row with *identical* content is a replay, not
    /// a conflict: it must skip — no overwrite, no history row, no warning —
    /// even when it carries a different `device_id`.
    #[test]
    fn equal_modify_time_identical_content_is_noop() {
        use datum_core::integrity::{IntegrityKind, ListFilter};

        let db = fresh_db();
        let local = make_entry("coffee", 0);
        let hid = local.hid.clone();
        db.insert_entry(&local).unwrap();

        let mut replay = make_entry("coffee", 0);
        replay.device_id = "dev-2".to_string();
        let cs = {
            let tmp = fresh_db();
            tmp.insert_entry(&replay).unwrap();
            pack_changeset(&tmp, 0).unwrap()
        };
        let before = history::list(db.conn(), &hid, false).unwrap().len();
        apply_changeset(&db, &cs).unwrap();

        let landed = db.get_entry(&local.slug).unwrap();
        assert_eq!(landed.device_id, "dev-1", "identical-content tie must skip");
        assert_eq!(
            history::list(db.conn(), &hid, false).unwrap().len(),
            before,
            "replay must not add a history row"
        );
        let warnings = db
            .list_integrity_warnings(&ListFilter {
                kind: Some(IntegrityKind::LwwOverwrite),
                pending: Some(true),
            })
            .unwrap();
        assert!(warnings.is_empty(), "replay must not file a warning");
    }

    /// A second apply of the exact same payload is a LWW no-op: no new
    /// history row.
    #[test]
    fn sync_merge_noop_does_not_create_history_row() {
        let db = fresh_db();
        let entry = make_entry("coffee", 0);
        let hid = entry.hid.clone();

        let cs = {
            let tmp = fresh_db();
            tmp.insert_entry(&entry).unwrap();
            pack_changeset(&tmp, 0).unwrap()
        };
        apply_changeset(&db, &cs).unwrap();

        let before = history::list(db.conn(), &hid, false).unwrap();
        let before_count = before.len();

        apply_changeset(&db, &cs).unwrap();

        let after = history::list(db.conn(), &hid, false).unwrap();
        assert_eq!(
            after.len(),
            before_count,
            "no-op sync_merge must not add a history row"
        );
    }

    /// An apply that strictly advances modify_time writes a sync_merge row
    /// snapshotting the pre-merge state.
    #[test]
    fn sync_merge_nontrivial_creates_history_row() {
        let db = fresh_db();
        let mut older = make_entry("coffee", 0);
        let mut newer = make_entry("coffee", 0);
        newer.modify_time = older.modify_time + Duration::seconds(60);
        // Distinguish older vs newer by real content; see
        // `apply_changeset_lww_skips_older_incoming` for the rationale.
        older.state = Some(serde_json::json!("older"));
        newer.state = Some(serde_json::json!("newer"));
        let hid = older.hid.clone();

        db.insert_entry(&older).unwrap();
        let versions_before = history::list(db.conn(), &hid, false).unwrap();
        assert_eq!(versions_before.len(), 1);

        let newer_cs = {
            let tmp = fresh_db();
            tmp.insert_entry(&newer).unwrap();
            pack_changeset(&tmp, 0).unwrap()
        };
        apply_changeset(&db, &newer_cs).unwrap();

        let versions_after = history::list(db.conn(), &hid, false).unwrap();
        assert_eq!(versions_after.len(), 2);
        assert_eq!(
            versions_after[1].change_kind,
            history::ChangeKind::SyncMerge
        );
        assert_eq!(
            versions_after[1].snapshot.state,
            Some(serde_json::json!("older")),
        );

        let landed = db.get_entry(&newer.slug).unwrap();
        assert_eq!(landed.state, Some(serde_json::json!("newer")));
    }

    /// A non-no-op LWW overwrite files exactly one `lww_overwrite`
    /// integrity warning: `local_entry` is the overwritten loser,
    /// `remote_entry` the winning incoming edit, tagged `sync_pull`.
    #[test]
    fn lww_overwrite_files_integrity_warning() {
        use datum_core::integrity::{self, DetectedBy, IntegrityKind, ListFilter};

        let db = fresh_db();
        let mut older = make_entry("coffee", 0);
        let mut newer = make_entry("coffee", 0);
        newer.modify_time = older.modify_time + Duration::seconds(60);
        older.state = Some(serde_json::json!("older"));
        newer.state = Some(serde_json::json!("newer"));

        db.insert_entry(&older).unwrap();

        let newer_cs = {
            let tmp = fresh_db();
            tmp.insert_entry(&newer).unwrap();
            pack_changeset(&tmp, 0).unwrap()
        };
        apply_changeset(&db, &newer_cs).unwrap();

        let warnings = integrity::list(
            db.conn(),
            &ListFilter {
                kind: Some(IntegrityKind::LwwOverwrite),
                pending: Some(true),
            },
        )
        .unwrap();
        assert_eq!(warnings.len(), 1, "one warning per overwrite");
        let w = &warnings[0];
        assert_eq!(w.detected_by, DetectedBy::SyncPull);
        assert_eq!(w.local_entry.state, Some(serde_json::json!("older")));
        assert_eq!(
            w.remote_entry.as_ref().unwrap().state,
            Some(serde_json::json!("newer")),
        );
    }

    /// A fresh slug arriving from a peer replaces no local edit, so it is
    /// not an overwrite and files no `lww_overwrite` warning. An LWW no-op
    /// (re-applying a row we already have) likewise files nothing.
    #[test]
    fn non_overwrites_file_no_warning() {
        use datum_core::integrity::{IntegrityKind, ListFilter};

        let db = fresh_db();
        let cs = {
            let tmp = fresh_db();
            tmp.insert_entry(&make_entry("coffee", 0)).unwrap();
            pack_changeset(&tmp, 0).unwrap()
        };
        // First apply = fresh arrival; second apply = LWW no-op.
        apply_changeset(&db, &cs).unwrap();
        apply_changeset(&db, &cs).unwrap();

        let warnings = db
            .list_integrity_warnings(&ListFilter {
                kind: Some(IntegrityKind::LwwOverwrite),
                pending: Some(true),
            })
            .unwrap();
        assert!(
            warnings.is_empty(),
            "neither fresh nor no-op is an overwrite"
        );
    }
}
