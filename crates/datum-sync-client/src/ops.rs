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
//!   path uses [`pack_changeset_page`] to walk that history one bounded
//!   page at a time, advancing the watermark to the max metric of exactly
//!   the rows each page packed. This is an epoch-ms value, NOT a
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

/// Entries per push page when the caller doesn't pick a size (da-5ua6).
/// A page is one HTTP request body, so this is the knob that keeps a
/// first sync of a long history under the relay's body limit
/// ([`datum_sync_server`'s `MAX_BODY_BYTES`], 8 MiB): ~1k entries is a
/// few hundred KB of JSON before encryption and base64.
pub const DEFAULT_PUSH_PAGE_ENTRIES: usize = 1000;

/// Pack every entry (including tombstones) whose `modified_at` epoch-ms is
/// strictly greater than `since_modify_ms`. Passing `0` produces a full dump
/// — unbounded, so this is for tests and callers that know the set is
/// small; the push path pages via [`pack_changeset_page`].
///
/// The returned changeset is self-contained plaintext bytes — the caller
/// is expected to pipe it through
/// [`crate::crypto::changeset::encrypt_changeset`] before it leaves the
/// device.
pub fn pack_changeset(db: &Db, since_modify_ms: u64) -> Result<Changeset> {
    Ok(pack_changeset_page(db, since_modify_ms, usize::MAX)?.0)
}

/// Pack at most `limit` entries newer than `since_modify_ms`, and return
/// the largest `modify_time_ms` (epoch-ms metric) among them — the value
/// the caller advances the push watermark to, and passes back as the next
/// `since_modify_ms` to walk the rest. Both the row filter and this
/// maximum read the same `modify_time_ms` generated column in a single
/// query, so the watermark exactly matches the packed set (no
/// chrono-vs-SQL drift, no TOCTOU against a concurrent writer) and
/// re-using it as the next `since_modify_ms` cannot skip a row — see
/// [`select_entries_since`] for how the page boundary stays honest when
/// several rows share one millisecond. Returns `None` when nothing was
/// packed, which is the loop's termination signal.
pub fn pack_changeset_page(
    db: &Db,
    since_modify_ms: u64,
    limit: usize,
) -> Result<(Changeset, Option<u64>)> {
    let (entries, max_ms) = select_entries_since(db, since_modify_ms, limit)?;
    let payload = ChangesetPayloadV1 { entries };
    let json = serde_json::to_vec(&payload)?;
    let mut bytes = Vec::with_capacity(CHANGESET_MAGIC_V1.len() + json.len());
    bytes.extend_from_slice(CHANGESET_MAGIC_V1);
    bytes.extend_from_slice(&json);
    Ok((Changeset::from_bytes(bytes), max_ms))
}

/// Apply a received changeset to the local DB using last-writer-wins on
/// `modify_time`, keyed on `hid` — the primary key, and the only identity
/// an entry keeps across an edit (`retime` rewrites the slug in place).
/// Existing rows whose stored `modify_time` is greater than the incoming
/// row's are left untouched; exact ties are broken by a deterministic total
/// order on `(content_hash, device_id)` so every device resolves the same
/// winner. Two *different* entries contending for one slug, or one live
/// `(field, occur_time)`, are both kept — the loser is parked and filed as
/// an `identity_collision` warning (see [`upsert_lww`]).
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

/// Select at most `limit` entries whose `modify_time_ms` exceeds
/// `since_ms`, returning the rows and the maximum `modify_time_ms` among
/// them.
///
/// The watermark filter is pushed into SQL against the indexed
/// `modify_time_ms` generated column (da-x3da), so this is a b-tree range
/// seek rather than a full-table scan + Rust-side filter. `since_ms == 0`
/// still selects every live/tombstoned row (a full dump) up to `limit`.
/// The max is taken from the same query so it is exactly the largest
/// metric in the packed set.
///
/// # Page boundaries and same-millisecond groups
///
/// The cursor has millisecond granularity, so the next page asks for
/// `> max_ms`: a page that ended *inside* a group of rows sharing one
/// `modify_time_ms` would silently skip that group's remainder. To see the
/// boundary, the query reads one row beyond the page. When that extra row
/// belongs to a new millisecond the page is already group-aligned and is
/// returned in full; when it continues the page's last group, that group
/// is dropped and comes back whole next time. A group that fills the page
/// on its own can't be dropped — that would make no progress — so it is
/// re-selected unbounded: an ms-granular cursor cannot split it, and one
/// oversized page beats a stalled loop or a skipped row.
fn select_entries_since(db: &Db, since_ms: u64, limit: usize) -> Result<(Vec<Entry>, Option<u64>)> {
    let mut rows = query_entries(
        db,
        "SELECT *, modify_time_ms AS _mtms FROM entries \
         WHERE modify_time_ms > ?1 ORDER BY modify_time_ms LIMIT ?2",
        params![since_ms as i64, clamp_limit(limit.saturating_add(1))],
    )?;

    if rows.len() > limit {
        let boundary_ms = rows[limit - 1].1;
        if rows[limit].1 != boundary_ms {
            rows.truncate(limit);
        } else if rows[0].1 == boundary_ms {
            rows = query_entries(
                db,
                "SELECT *, modify_time_ms AS _mtms FROM entries \
                 WHERE modify_time_ms = ?1",
                params![boundary_ms],
            )?;
        } else {
            let cut = rows.iter().position(|(_, ms)| *ms == boundary_ms).unwrap();
            rows.truncate(cut);
        }
    }

    let mut max_ms: Option<u64> = None;
    let mut out = Vec::with_capacity(rows.len());
    for (entry, ms) in rows {
        if ms >= 0 {
            let ms = ms as u64;
            max_ms = Some(max_ms.map_or(ms, |cur| cur.max(ms)));
        }
        out.push(entry);
    }
    Ok((out, max_ms))
}

/// `usize::MAX` as an i64 LIMIT would wrap to -1, which SQLite reads as
/// "no limit" — accidentally correct for [`pack_changeset`], but not
/// something to rely on. Saturate instead.
fn clamp_limit(limit: usize) -> i64 {
    limit.min(i64::MAX as usize) as i64
}

/// Run one of the two page selections, pairing each entry with its
/// `modify_time_ms`. `modify_time_ms` is aliased so we can read it by a
/// stable name regardless of whether `SELECT *` surfaces the generated
/// column on this SQLite build.
fn query_entries(db: &Db, sql: &str, args: impl rusqlite::Params) -> Result<Vec<(Entry, i64)>> {
    let mut stmt = db.conn().prepare(sql)?;
    let rows = stmt.query_map(args, |row| {
        let entry = datum_core::db::row_to_entry_public(row)?;
        let ms: i64 = row.get("_mtms")?;
        Ok((entry, ms))
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// Deterministic total order used to pick a winner between two rows that
/// cannot both hold one identity. Extends the equal-`modify_time`
/// tiebreaker (`(content_hash, device_id)`, part of the wire contract)
/// with `hid` as a final discriminator so the order is *total* — two rows
/// with identical content from the same device still resolve, and every
/// device resolves them the same way. Greater wins.
fn merge_rank(e: &Entry) -> (chrono::DateTime<chrono::Utc>, &str, &str, &str) {
    (e.modify_time, &e.content_hash, &e.device_id, e.hid.as_str())
}

/// The slug a parked loser takes: its own slug suffixed with its own hid.
/// Deterministic (no RNG — unlike `insert.rs`'s local `suffixed_slug`) and
/// unique (hid is the primary key), which is what lets two devices reach
/// the same pair of slugs without talking to each other.
fn parked_slug(loser: &Entry) -> String {
    format!("{}_{}", loser.slug, loser.hid)
}

/// Local rows, other than `entry` itself, that hold an identity `entry`
/// needs: the same slug (total UNIQUE, tombstones included), or the same
/// live `(field, occur_time)` (partial UNIQUE, live rows only). This is
/// the pre-check the merge's identity probe has to be in order to be one —
/// every constraint the write can trip is looked up here, so the apply
/// decides a merge rather than meeting a raw SQLite error (da-zlug).
fn find_blockers(tx: &rusqlite::Transaction<'_>, entry: &Entry) -> Result<Vec<Entry>> {
    let mut blockers: Vec<Entry> = tx
        .prepare("SELECT * FROM entries WHERE slug = ?1 AND hid != ?2")?
        .query_map(
            params![entry.slug, entry.hid],
            datum_core::db::row_to_entry_public,
        )?
        .collect::<rusqlite::Result<_>>()?;

    // The partial index only covers live rows with a real occur_time, so an
    // incoming tombstone or a timeless entry can never trip it.
    if entry.deleted_at.is_none() {
        if let Some(occur) = entry.occur_time.as_ref() {
            let rows: Vec<Entry> = tx
                .prepare(
                    "SELECT * FROM entries \
                     WHERE field = ?1 AND occur_time = ?2 \
                       AND deleted_at IS NULL AND hid != ?3",
                )?
                .query_map(
                    params![entry.field, occur.utc.to_rfc3339(), entry.hid],
                    datum_core::db::row_to_entry_public,
                )?
                .collect::<rusqlite::Result<_>>()?;
            for row in rows {
                if !blockers.iter().any(|b| b.hid == row.hid) {
                    blockers.push(row);
                }
            }
        }
    }
    Ok(blockers)
}

/// Park `loser` out of `winner`'s way without destroying it: move it to its
/// [`parked_slug`] when the two contend for one slug, and tombstone it
/// (`deleted_at` = the winner's `modify_time`, so both devices write the
/// same value) when they contend for one live `(field, occur_time)`.
///
/// `modify_time` is deliberately NOT bumped. Parking is a pure function of
/// the two rows, so every device reaches it independently; a bump would
/// both re-push a row nobody edited and let the parked copy start winning
/// LWW against its own origin.
///
/// `write_now` distinguishes the two losers: a local blocker is already a
/// row and is rewritten here, while a losing *incoming* row is returned for
/// the caller's own write. Either way the returned entry is the parked
/// shape, and it is what the warning names.
fn park_loser(
    tx: &rusqlite::Transaction<'_>,
    loser: &Entry,
    winner: &Entry,
    write_now: bool,
) -> Result<Entry> {
    let mut parked = loser.clone();
    if loser.slug == winner.slug {
        parked.slug = datum_core::ids::Slug::new(parked_slug(loser));
    }
    if loser.deleted_at.is_none()
        && winner.deleted_at.is_none()
        && loser.occur_time.is_some()
        && loser.field == winner.field
        && loser.occur_time.as_ref().map(|t| t.utc) == winner.occur_time.as_ref().map(|t| t.utc)
    {
        parked.deleted_at = Some(winner.modify_time);
    }
    if write_now {
        // The loser is a row already in the table: rewrite just the two
        // identity columns the parking touches, snapshotting the pre-park
        // state so the move is auditable and reversible.
        history::record(tx, loser, ChangeKind::SyncMerge)?;
        tx.execute(
            "UPDATE entries SET slug = ?2, deleted_at = ?3 WHERE hid = ?1",
            params![
                parked.hid,
                parked.slug,
                parked.deleted_at.map(|t| t.to_rfc3339()),
            ],
        )?;
    }
    datum_core::integrity::file_identity_collision(tx, &parked, winner)?;
    Ok(parked)
}

fn upsert_lww(tx: &rusqlite::Transaction<'_>, entry: &Entry) -> Result<()> {
    // The merge keys on `hid` — the primary key, and the only identity a
    // row keeps across an edit. `slug` is derived from `field` + occur_time
    // and `retime` rewrites it in place (datum-core/src/db/time.rs:147), so
    // a slug-keyed probe misses a retimed row's earlier changeset and
    // "freshly" inserts it onto its own primary key (da-zlug).
    //
    // Pull the current full row so we can snapshot it into entry_history
    // when an apply actually changes state — no-op LWW short-circuits do
    // NOT write history.
    let existing: Option<Entry> = tx
        .query_row(
            "SELECT * FROM entries WHERE hid = ?1",
            params![entry.hid],
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
        // Only this branch is an overwrite — a row arriving under an hid we
        // don't hold replaces nothing local and files no warning.
        history::record(tx, pre, ChangeKind::SyncMerge)?;
        datum_core::integrity::file_lww_overwrite(tx, pre, entry)?;
    }

    // `hid` merge means two *different* rows can now contend for one slug
    // or one live `(field, occur_time)` — the constraints slug-keyed merge
    // used to hide by collapsing both sides into one row. Neither is a
    // crash and neither is a discard: keep both, park the deterministic
    // loser, and file a warning naming the pair (da-zlug). The policy for
    // deduping an *essentially identical* pair silently is da-yiq9, which
    // waits on `content_hash` covering `following_state`.
    let blockers = find_blockers(tx, entry)?;
    let top = blockers
        .iter()
        .max_by(|a, b| merge_rank(a).cmp(&merge_rank(b)));
    let parked;
    let entry = match top {
        // Incoming loses: write it in its parked shape rather than dropping
        // it, so the device that pulls second holds the same rows as the one
        // that pulled first.
        Some(winner) if merge_rank(winner) > merge_rank(entry) => {
            parked = park_loser(tx, entry, winner, false)?;
            &parked
        }
        // Incoming wins: move every blocker out of the way in place.
        _ => {
            for blocker in &blockers {
                park_loser(tx, blocker, entry, true)?;
            }
            entry
        }
    };

    if existing.is_none() {
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
    let occur_tz = entry.occur_time.as_ref().and_then(|t| t.timezone.clone());
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
        ON CONFLICT(hid) DO UPDATE SET
            -- Merge keys on hid — the primary key, replicated with
            -- fidelity, and the identity entry_history, integrity_warnings
            -- and prefix-addressing already join on. `slug` is derived from
            -- field + occur_time and `retime` rewrites it in place, so it
            -- is the *mutable* column here and takes the winner's value
            -- (da-zlug). The slug/(field, occur_time) constraints are
            -- pre-resolved above, so hid is the only conflict left to hit.
            slug            = excluded.slug,
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

    /// Pack one entry as a peer would ship it.
    fn changeset_of(entry: &Entry) -> Changeset {
        let tmp = fresh_db();
        tmp.insert_entry(entry).unwrap();
        pack_changeset(&tmp, 0).unwrap()
    }

    /// The row stored under `hid`, live or tombstoned.
    fn row_by_hid(db: &Db, hid: &datum_core::ids::Hid) -> Entry {
        db.conn()
            .query_row(
                "SELECT * FROM entries WHERE hid = ?1",
                params![hid],
                datum_core::db::row_to_entry_public,
            )
            .unwrap()
    }

    fn make_entry(field: &str, offset_secs: i64) -> Entry {
        let base = Utc.with_ymd_and_hms(2026, 4, 21, 12, 0, 0).unwrap();
        let at = base + Duration::seconds(offset_secs);
        let t = DatumTime {
            utc: at,
            offset_minutes: 0,
            timezone: Some("UTC".to_string()),
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

        let (cs, max_ms) = pack_changeset_page(&db, 0, usize::MAX).unwrap();
        assert_eq!(parse_payload(cs.as_bytes()).unwrap().entries.len(), 2);
        let watermark = max_ms.expect("packed rows yield a watermark");

        let (cs2, max2) = pack_changeset_page(&db, watermark, usize::MAX).unwrap();
        assert_eq!(
            parse_payload(cs2.as_bytes()).unwrap().entries.len(),
            0,
            "rows at or below the advanced watermark must not re-pack"
        );
        assert_eq!(max2, None);
    }

    /// Walk a history the way `push_once` does — page, advance the
    /// watermark to the returned cursor, repeat — and land every row
    /// exactly once, in bounded pages (da-5ua6).
    fn walk_pages(db: &Db, limit: usize) -> (Vec<usize>, Vec<String>) {
        let mut sizes = Vec::new();
        let mut slugs = Vec::new();
        let mut watermark = 0u64;
        loop {
            let (cs, max_ms) = pack_changeset_page(db, watermark, limit).unwrap();
            let entries = parse_payload(cs.as_bytes()).unwrap().entries;
            if entries.is_empty() {
                break;
            }
            sizes.push(entries.len());
            slugs.extend(entries.into_iter().map(|e| e.slug.to_string()));
            watermark = max_ms.expect("a non-empty page yields a cursor");
        }
        (sizes, slugs)
    }

    #[test]
    fn paged_walk_covers_every_row_exactly_once() {
        let db = fresh_db();
        // Distinct seconds → distinct modify_time_ms, so pages fill exactly.
        for i in 0..10 {
            db.insert_entry(&make_entry(&format!("f{i}"), i * 60))
                .unwrap();
        }

        let (sizes, mut slugs) = walk_pages(&db, 3);
        assert_eq!(sizes, vec![3, 3, 3, 1], "pages must respect the limit");
        slugs.sort();
        slugs.dedup();
        assert_eq!(slugs.len(), 10, "every row packed exactly once");
    }

    /// The cursor has ms granularity, so a page must never end mid-group:
    /// the next page asks for `> max_ms` and would skip the remainder.
    /// A full page drops its trailing group; the group returns whole next
    /// time.
    #[test]
    fn page_boundary_never_splits_a_millisecond_group() {
        let db = fresh_db();
        // Three rows at t+0 (one shared ms), two at t+60.
        for f in ["a", "b", "c"] {
            db.insert_entry(&make_entry(f, 0)).unwrap();
        }
        for f in ["d", "e"] {
            db.insert_entry(&make_entry(f, 60)).unwrap();
        }

        // limit 4 would cut the t+60 group in half — it must be deferred.
        let (sizes, mut slugs) = walk_pages(&db, 4);
        assert_eq!(sizes, vec![3, 2]);
        slugs.sort();
        assert_eq!(slugs.len(), 5, "no row skipped across the boundary");
    }

    /// A single millisecond holding more rows than the page size cannot be
    /// split by an ms-granular cursor, so it ships whole — an oversized
    /// page, but never a skipped row or a stalled loop.
    #[test]
    fn oversized_millisecond_group_ships_whole() {
        let db = fresh_db();
        for f in ["a", "b", "c", "d"] {
            db.insert_entry(&make_entry(f, 0)).unwrap();
        }
        db.insert_entry(&make_entry("later", 60)).unwrap();

        let (sizes, slugs) = walk_pages(&db, 2);
        assert_eq!(sizes, vec![4, 1], "the 4-row group exceeds the page whole");
        assert_eq!(slugs.len(), 5);
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
        // One entry edited on two devices: since da-zlug the merge keys on
        // hid, so "the same entry" means the same hid.
        newer.hid = older.hid.clone();
        newer.modify_time = older.modify_time + Duration::seconds(60);
        // Distinguish older vs newer by real content — the storage layer
        // canonicalizes content_hash (da-r569), so a synthetic label on
        // `entry.content_hash` no longer survives the insert.
        newer.state = Some(serde_json::json!("newer"));
        older.state = Some(serde_json::json!("older"));

        db_a.insert_entry(&newer).unwrap();

        let older_cs = changeset_of(&older);
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

        // One entry (one hid), edited on each device in the same instant.
        let mut on_a = make_entry("coffee", 0);
        let mut on_b = make_entry("coffee", 0);
        on_b.hid = on_a.hid.clone();
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
        replay.hid = hid.clone();
        replay.device_id = "dev-2".to_string();
        let cs = changeset_of(&replay);
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
        newer.hid = older.hid.clone();
        newer.modify_time = older.modify_time + Duration::seconds(60);
        // Distinguish older vs newer by real content; see
        // `apply_changeset_lww_skips_older_incoming` for the rationale.
        older.state = Some(serde_json::json!("older"));
        newer.state = Some(serde_json::json!("newer"));
        let hid = older.hid.clone();

        db.insert_entry(&older).unwrap();
        let versions_before = history::list(db.conn(), &hid, false).unwrap();
        assert_eq!(versions_before.len(), 1);

        let newer_cs = changeset_of(&newer);
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
        newer.hid = older.hid.clone();
        newer.modify_time = older.modify_time + Duration::seconds(60);
        older.state = Some(serde_json::json!("older"));
        newer.state = Some(serde_json::json!("newer"));

        db.insert_entry(&older).unwrap();

        let newer_cs = changeset_of(&newer);
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

    /// da-zlug, the reported crash: `retime` rekeys a row in place — hid
    /// preserved, slug rewritten (`datum-core/src/db/time.rs:147`) — and the
    /// relay is append-only, so the entry sits on it under two slugs with
    /// one hid. A slug-keyed probe missed the second one and inserted it
    /// straight onto its own primary key: `UNIQUE constraint failed:
    /// entries.hid`. Merging on hid makes it the update it always was.
    #[test]
    fn retimed_entry_replays_onto_itself_without_pk_collision() {
        let db = fresh_db();
        let before = make_entry("coffee", 0);
        let mut after = make_entry("coffee", 3600);
        after.hid = before.hid.clone();
        after.modify_time = before.modify_time + Duration::seconds(60);
        assert_ne!(before.slug, after.slug, "retime changes the slug");

        // The relay replays the pre-retime changeset, then the post-retime
        // one — both live there forever.
        apply_changeset(&db, &changeset_of(&before)).unwrap();
        apply_changeset(&db, &changeset_of(&after)).unwrap();

        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1, "one entry, not two — the hid is the identity");
        let landed = row_by_hid(&db, &before.hid);
        assert_eq!(landed.slug, after.slug, "the row follows the new slug");
        assert!(
            db.get_entry(&before.slug).is_err(),
            "the pre-retime slug no longer resolves"
        );
        // History stays joined to the hid, which nothing rewrites (da-0ye9).
        assert_eq!(
            history::list(db.conn(), &before.hid, false).unwrap().len(),
            2
        );
    }

    /// The other half of the inversion: two *different* entries claiming one
    /// slug. Both are kept — the deterministic loser is parked under a slug
    /// carrying its own hid, and tombstoned because a live pair cannot share
    /// `(field, occur_time)` — and one `identity_collision` warning names
    /// the pair for `datum check` (da-zlug; policy refinement is da-yiq9).
    #[test]
    fn same_slug_different_hid_keeps_both_and_parks_the_loser() {
        use datum_core::integrity::{DetectedBy, IntegrityKind, ListFilter};

        let db = fresh_db();
        let mut local = make_entry("coffee", 0);
        let mut incoming = make_entry("coffee", 0);
        local.state = Some(serde_json::json!("local"));
        incoming.state = Some(serde_json::json!("incoming"));
        incoming.modify_time = local.modify_time + Duration::seconds(60);
        assert_eq!(local.slug, incoming.slug);
        assert_ne!(local.hid, incoming.hid);

        db.insert_entry(&local).unwrap();
        apply_changeset(&db, &changeset_of(&incoming)).unwrap();

        // Newer incoming wins the contested slug; the local row survives.
        let kept = db.get_entry(&incoming.slug).unwrap();
        assert_eq!(kept.hid, incoming.hid);
        let parked = row_by_hid(&db, &local.hid);
        assert_eq!(
            parked.slug.as_str(),
            format!("{}_{}", local.slug, local.hid)
        );
        assert_eq!(
            parked.deleted_at,
            Some(incoming.modify_time),
            "tombstoned at the winner's modify_time — the same value on every device"
        );
        assert_eq!(
            parked.state,
            Some(serde_json::json!("local")),
            "not destroyed"
        );

        let warnings = db
            .list_integrity_warnings(&ListFilter {
                kind: Some(IntegrityKind::IdentityCollision),
                pending: Some(true),
            })
            .unwrap();
        assert_eq!(warnings.len(), 1, "one warning naming the pair");
        assert_eq!(warnings[0].detected_by, DetectedBy::SyncPull);
        assert_eq!(warnings[0].local_entry.hid, local.hid);
        assert_eq!(warnings[0].remote_entry.as_ref().unwrap().hid, incoming.hid);
    }

    /// The parking rule is a pure function of the two rows, so devices that
    /// meet them in opposite orders land on the same pair of rows — the
    /// divergence guard da-22zm names, applied to the collision the
    /// inversion creates. Re-applying afterwards changes nothing.
    #[test]
    fn identity_collision_converges_regardless_of_arrival_order() {
        use datum_core::integrity::{IntegrityKind, ListFilter};

        let mut on_a = make_entry("coffee", 0);
        let mut on_b = make_entry("coffee", 0);
        on_a.state = Some(serde_json::json!("from-a"));
        on_b.state = Some(serde_json::json!("from-b"));
        on_b.device_id = "dev-2".to_string();
        on_b.modify_time = on_a.modify_time + Duration::seconds(60);
        let cs_a = changeset_of(&on_a);
        let cs_b = changeset_of(&on_b);

        let db_a = fresh_db();
        apply_changeset(&db_a, &cs_a).unwrap();
        apply_changeset(&db_a, &cs_b).unwrap();
        let db_b = fresh_db();
        apply_changeset(&db_b, &cs_b).unwrap();
        apply_changeset(&db_b, &cs_a).unwrap();
        // ...and a re-pull of both, which the append-only relay guarantees.
        apply_changeset(&db_a, &cs_a).unwrap();
        apply_changeset(&db_b, &cs_b).unwrap();

        let shape = |db: &Db| -> Vec<(String, String, bool)> {
            let mut stmt = db
                .conn()
                .prepare("SELECT hid, slug, deleted_at FROM entries ORDER BY hid")
                .unwrap();
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?.is_some(),
                    ))
                })
                .unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        let shape_a = shape(&db_a);
        assert_eq!(shape_a.len(), 2, "both rows kept");
        assert_eq!(shape_a, shape(&db_b), "same two rows, same two slugs");

        // Idempotent: the second pass files no further warnings.
        let count = |db: &Db| {
            db.list_integrity_warnings(&ListFilter {
                kind: Some(IntegrityKind::IdentityCollision),
                pending: Some(true),
            })
            .unwrap()
            .len()
        };
        assert_eq!(count(&db_a), 1);
        assert_eq!(count(&db_b), 1);
    }

    /// The third constraint, which no test ever built: two rows with
    /// *distinct* slugs (custom `--id` templates) at the same field and
    /// instant trip the partial UNIQUE on `(field, occur_time)`. It is a
    /// merge case like the others, not a constraint error — the loser keeps
    /// its own slug and is tombstoned.
    #[test]
    fn field_occur_time_collision_is_a_merge_not_an_error() {
        use datum_core::integrity::{IntegrityKind, ListFilter};

        let db = fresh_db();
        let mut local = make_entry("coffee", 0);
        let mut incoming = make_entry("coffee", 0);
        local.slug = datum_core::ids::Slug::new("coffee:morning");
        incoming.slug = datum_core::ids::Slug::new("coffee:first-cup");
        incoming.modify_time = local.modify_time + Duration::seconds(60);

        db.insert_entry(&local).unwrap();
        apply_changeset(&db, &changeset_of(&incoming)).unwrap();

        let kept = db.get_entry(&incoming.slug).unwrap();
        assert_eq!(kept.hid, incoming.hid);
        let parked = row_by_hid(&db, &local.hid);
        assert_eq!(parked.slug, local.slug, "no slug contest, no slug change");
        assert_eq!(parked.deleted_at, Some(incoming.modify_time));
        assert_eq!(
            db.list_integrity_warnings(&ListFilter {
                kind: Some(IntegrityKind::IdentityCollision),
                pending: Some(true),
            })
            .unwrap()
            .len(),
            1,
        );
    }

    /// An incoming row that *loses* the contest is still applied — parked
    /// rather than dropped, so the two devices hold the same rows and the
    /// entry is not silently lost on whichever device happens to be second.
    #[test]
    fn losing_incoming_row_is_parked_not_discarded() {
        let db = fresh_db();
        let mut local = make_entry("coffee", 0);
        let mut incoming = make_entry("coffee", 0);
        local.state = Some(serde_json::json!("local"));
        incoming.state = Some(serde_json::json!("incoming"));
        local.modify_time = incoming.modify_time + Duration::seconds(60);

        db.insert_entry(&local).unwrap();
        apply_changeset(&db, &changeset_of(&incoming)).unwrap();

        assert_eq!(db.get_entry(&local.slug).unwrap().hid, local.hid);
        let parked = row_by_hid(&db, &incoming.hid);
        assert_eq!(
            parked.slug.as_str(),
            format!("{}_{}", incoming.slug, incoming.hid)
        );
        assert_eq!(parked.deleted_at, Some(local.modify_time));
        assert_eq!(parked.state, Some(serde_json::json!("incoming")));
    }
}
