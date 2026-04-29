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
//! - `since_seq` in [`pack_changeset`] is a local watermark: the largest
//!   `modified_at` epoch-ms already included in a prior pack. Passing `0`
//!   produces a full dump of every live and tombstoned entry.
//! - [`crate::sync_state::advance_push_seq`] /
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
/// strictly greater than `since_seq`. Passing `0` produces a full dump.
///
/// The returned changeset is self-contained plaintext bytes — the caller
/// is expected to pipe it through
/// [`crate::crypto::changeset::encrypt_changeset`] before it leaves the
/// device.
pub fn pack_changeset(db: &Db, since_seq: u64) -> Result<Changeset> {
    let entries = select_entries_since(db, since_seq)?;
    let payload = ChangesetPayloadV1 { entries };
    let json = serde_json::to_vec(&payload)?;
    let mut bytes = Vec::with_capacity(CHANGESET_MAGIC_V1.len() + json.len());
    bytes.extend_from_slice(CHANGESET_MAGIC_V1);
    bytes.extend_from_slice(&json);
    Ok(Changeset::from_bytes(bytes))
}

/// Apply a received changeset to the local DB using last-writer-wins on
/// `modified_at`. Existing rows whose stored `modified_at` is >= the
/// incoming row's `modified_at` are left untouched.
pub fn apply_changeset(db: &Db, cs: &Changeset) -> Result<()> {
    let payload = parse_payload(cs.as_bytes())?;
    let tx = db.conn().unchecked_transaction()?;
    for entry in payload.entries.iter() {
        upsert_lww(&tx, entry)?;
    }
    tx.commit()?;
    Ok(())
}

fn parse_payload(bytes: &[u8]) -> Result<ChangesetPayloadV1> {
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
    let payload: ChangesetPayloadV1 = serde_json::from_slice(rest)?;
    Ok(payload)
}

fn select_entries_since(db: &Db, since_ms: u64) -> Result<Vec<Entry>> {
    // Fetch candidate rows and filter by epoch-ms in Rust — the stored
    // `modify_time` is RFC3339, so a pure-SQL comparison would require a
    // cast-and-parse dance that's more fragile than a one-pass filter.
    // Use `SELECT *` so the generated `verb` / `dur_seconds` columns are
    // included without listing them explicitly.
    let mut stmt = db.conn().prepare(
        "SELECT * FROM entries ORDER BY modify_time",
    )?;
    let rows = stmt.query_map([], datum_core::db::row_to_entry_public)?;
    let mut out = Vec::new();
    for row in rows {
        let entry = row?;
        if entry.modify_time.timestamp_millis() as u64 > since_ms {
            out.push(entry);
        }
    }
    Ok(out)
}

fn upsert_lww(tx: &rusqlite::Transaction<'_>, entry: &Entry) -> Result<()> {
    // Skip rows we already have as-new-or-newer. Also pull the current
    // full-row so we can snapshot it into entry_history when an apply
    // actually changes state — no-op LWW short-circuits do NOT write history.
    let existing: Option<Entry> = tx
        .query_row(
            "SELECT * FROM entries WHERE slug = ?1",
            params![entry.slug],
            datum_core::db::row_to_entry_public,
        )
        .optional()?;
    if let Some(pre) = existing.as_ref() {
        if pre.modify_time >= entry.modify_time {
            return Ok(());
        }
        // Non-no-op: record the pre-merge state as a sync_merge history row.
        history::record(tx, pre, ChangeKind::SyncMerge)?;
    } else {
        // Fresh row arriving from a peer — the "pre" state is the incoming
        // row itself (same as a create). Record it as sync_merge so the
        // change_kind reflects how it got here, matching the protocol spec.
        history::record(tx, entry, ChangeKind::SyncMerge)?;
    }

    tx.execute(
        r#"INSERT INTO entries (
            hid, slug, content_hash, field, entry_shape, intent,
            value, data, unit,
            start_time, start_time_offset, start_time_tz,
            start_time_precision, start_time_precision_unit,
            end_time, end_time_offset, end_time_tz,
            end_time_precision, end_time_precision_unit,
            lat, lon, location,
            create_time, modify_time, device_id,
            deleted_at, id_structure, field_structure,
            raw_input, alias_chain, parser_version,
            extra_metadata
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6,
            ?7, ?8, ?9,
            ?10, ?11, ?12,
            ?13, ?14,
            ?15, ?16, ?17,
            ?18, ?19,
            ?20, ?21, ?22,
            ?23, ?24, ?25,
            ?26, ?27, ?28,
            ?29, ?30, ?31,
            ?32
        )
        ON CONFLICT(slug) DO UPDATE SET
            hid             = excluded.hid,
            content_hash    = excluded.content_hash,
            field           = excluded.field,
            entry_shape     = excluded.entry_shape,
            intent          = excluded.intent,
            value           = excluded.value,
            data            = excluded.data,
            unit            = excluded.unit,
            start_time      = excluded.start_time,
            start_time_offset = excluded.start_time_offset,
            start_time_tz   = excluded.start_time_tz,
            start_time_precision = excluded.start_time_precision,
            start_time_precision_unit = excluded.start_time_precision_unit,
            end_time        = excluded.end_time,
            end_time_offset = excluded.end_time_offset,
            end_time_tz     = excluded.end_time_tz,
            end_time_precision = excluded.end_time_precision,
            end_time_precision_unit = excluded.end_time_precision_unit,
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
            entry.entry_shape.to_string(),
            entry.intent,
            entry.value.as_ref().map(|v| v.to_string()),
            entry.data.as_ref().map(|v| v.to_string()),
            entry.unit,
            entry.start_time.as_ref().map(|t| t.utc.to_rfc3339()),
            entry.start_time.as_ref().map(|t| t.offset_minutes),
            entry.start_time.as_ref().map(|t| t.timezone.as_str()),
            entry.start_time_precision,
            entry.start_time_precision_unit.to_string(),
            entry.end_time.as_ref().map(|t| t.utc.to_rfc3339()),
            entry.end_time.as_ref().map(|t| t.offset_minutes),
            entry.end_time.as_ref().map(|t| t.timezone.as_str()),
            entry.end_time_precision,
            entry.end_time_precision_unit.to_string(),
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
            Verb::Occur,
            Some(serde_json::json!(1)),
            None,
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
    fn pack_since_seq_returns_only_new_rows() {
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

    #[test]
    fn apply_changeset_lww_skips_older_incoming() {
        let db_a = fresh_db();
        let db_b = fresh_db();

        let mut older = make_entry("coffee", 0);
        let mut newer = make_entry("coffee", 0);
        newer.modify_time = older.modify_time + Duration::seconds(60);
        newer.content_hash = "deadbeef".to_string();
        older.content_hash = "cafebabe".to_string();

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

        let hash: String = db_b
            .conn()
            .query_row(
                "SELECT content_hash FROM entries WHERE slug = ?1",
                params![newer.slug],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(hash, "deadbeef");
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
        older.content_hash = "cafebabe".to_string();
        newer.content_hash = "deadbeef".to_string();
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
        assert_eq!(versions_after[1].snapshot.content_hash, "cafebabe");

        let stored_hash: String = db
            .conn()
            .query_row(
                "SELECT content_hash FROM entries WHERE slug = ?1",
                params![newer.slug],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_hash, "deadbeef");
    }
}
