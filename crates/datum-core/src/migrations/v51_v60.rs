//! Migration bodies for schema versions 50 → 60 — the current era.
//!
//! New migrations land here. When `CURRENT_SCHEMA_VERSION` crosses 60,
//! start `v61_v70.rs` rather than growing this file.

use rusqlite::Connection;

use super::support::{has_column, table_exists};

use crate::db::KIT_FIELD;
use crate::error::{DatumError, Result};

/// da-xnzc7: `kit` is reserved for kits' `[kit]` tables, as `spec`, `alias`
/// and `view` / `command` / `index` were before it. Nothing moves; a database
/// with life data logged under `kit` is refused rather than having that data
/// hidden from every surface and its field made unwritable.
pub(super) fn m50_to_51_kit_is_reserved(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "entries")? {
        return Ok(());
    }
    let clashing: i64 = conn.query_row(
        // A deleted log is not in the way: nothing reads it as life data. Nor
        // is a kit's table itself — synced in from a v51 peer (sync carries
        // no schema version), or kept by the down.
        "SELECT COUNT(*) FROM entries WHERE field = ?1 AND deleted_at IS NULL
           AND NOT (slug = ?1 || ':' || json_extract(data, '$.name')
                    AND json_extract(data, '$.kit_id') IS NOT NULL)",
        [KIT_FIELD],
        |row| row.get(0),
    )?;
    if clashing > 0 {
        return Err(DatumError::Other(format!(
            "cannot reserve `{KIT_FIELD}` for kits: this database has {clashing} \
             entr{} of a field named `{KIT_FIELD}`. Rename that field with an older \
             datum first; nothing was changed.",
            if clashing == 1 { "y" } else { "ies" }
        )));
    }
    Ok(())
}

/// Reverse v50→v51. The `kit:<name>` entries stay: to a v50 binary they are
/// plain logs of a field named `kit`.
pub(super) fn down_51_to_50_kit_is_reserved(_conn: &Connection) -> Result<()> {
    Ok(())
}

/// da-z0zh3: the undo stack — `undo_actions`, and the action each
/// `entry_history` row was written by. Older rows have none, and undo stops
/// at them rather than guess how they grouped.
pub(super) fn m51_to_52_undo_actions(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "ALTER TABLE entry_history ADD COLUMN action_id INTEGER;
         CREATE INDEX IF NOT EXISTS entry_history_action
             ON entry_history(action_id) WHERE action_id IS NOT NULL;
         CREATE TABLE IF NOT EXISTS undo_actions (
             id        INTEGER PRIMARY KEY AUTOINCREMENT,
             time      TEXT NOT NULL,
             undone    INTEGER NOT NULL DEFAULT 0,
             untracked TEXT
         );",
    )?;
    Ok(())
}

/// Reverse v51→v52. Local: the stack is forgotten, the history stays.
pub(super) fn down_52_to_51_undo_actions(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP INDEX IF EXISTS entry_history_action;
         ALTER TABLE entry_history DROP COLUMN action_id;
         DROP TABLE IF EXISTS undo_actions;",
    )?;
    Ok(())
}

/// da-asgt4: `entries_live_field_occur` gains `slug`, the tie-break of the
/// state order (`occur_time DESC, slug DESC`), `entries_field_state`
/// indexes the rows that move a field's state, and `entries_field_states`
/// the states each field was logged with. Without them "the field's newest
/// state row before T, LIMIT 1" sorted every earlier row of the field (on a
/// field of occur dots alone, walked all of it to find none), and `datum
/// fields` read the whole table.
pub(super) fn m52_to_53_field_state_indexes(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "entries")? {
        return Ok(());
    }
    conn.execute_batch(
        "DROP INDEX IF EXISTS entries_live_field_occur;
         CREATE INDEX entries_live_field_occur
             ON entries (field, occur_time, slug) WHERE deleted_at IS NULL;
         CREATE INDEX IF NOT EXISTS entries_field_state
             ON entries (field, occur_time, slug)
             WHERE occur_time IS NOT NULL AND deleted_at IS NULL AND superseded_by IS NULL
               AND NOT (verb = 'point' AND (state IS NULL OR state = 'true'));
         CREATE INDEX IF NOT EXISTS entries_field_states
             ON entries (field, state)
             WHERE deleted_at IS NULL AND superseded_by IS NULL AND state IS NOT NULL
               AND json_extract(COALESCE(extra_metadata, '{}'), '$.track') IS NULL;",
    )?;
    Ok(())
}

/// da-89ujs: `ops_held` indexes the held ops (`applied = 0`), which every
/// apply asks for: without it the question read every op's body, the whole
/// table, and a browser's join paid that once per chunk on a cold page
/// cache. Partial, so that no query asking for applied ops can be drawn
/// onto it: a full index on `applied` was, and `MAX(hlc) … WHERE applied =
/// 1` walked every op on every write.
pub(super) fn m54_to_55_ops_held(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "ops")? {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS ops_held ON ops(endpoint, seq) WHERE applied = 0;",
    )?;
    Ok(())
}

/// Reverse v54→v55.
pub(super) fn down_55_to_54_ops_held(conn: &Connection) -> Result<()> {
    conn.execute_batch("DROP INDEX IF EXISTS ops_held;")?;
    Ok(())
}

/// Reverse v52→v53: the live index without `slug`, and no state indexes.
pub(super) fn down_53_to_52_field_state_indexes(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "entries")? {
        return Ok(());
    }
    conn.execute_batch(
        "DROP INDEX IF EXISTS entries_field_state;
         DROP INDEX IF EXISTS entries_field_states;
         DROP INDEX IF EXISTS entries_live_field_occur;
         CREATE INDEX entries_live_field_occur
             ON entries (field, occur_time) WHERE deleted_at IS NULL;",
    )?;
    Ok(())
}

/// da-38eye: `sync_peers.redeemed_at`, when this copy redeemed an invite
/// with the peer and was admitted. A join cut short before any op applied
/// holds nothing of its inviter's, so only this says where it resumes.
pub(super) fn m53_to_54_sync_peer_redeemed(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "sync_peers")? {
        return Ok(());
    }
    conn.execute_batch("ALTER TABLE sync_peers ADD COLUMN redeemed_at TEXT;")?;
    Ok(())
}

/// Reverse v53→v54. Local: the mark goes, the peers stay.
pub(super) fn down_54_to_53_sync_peer_redeemed(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "sync_peers")? {
        return Ok(());
    }
    conn.execute_batch("ALTER TABLE sync_peers DROP COLUMN redeemed_at;")?;
    Ok(())
}

/// da-n7tp2: `stats`' per-day cache, its triggers and the index they read.
/// It starts empty; the first `stats` fills it.
pub(super) fn m55_to_56_stats_cache(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "entries")? {
        return Ok(());
    }
    conn.execute_batch(crate::db::STATS_CACHE_SQL)?;
    Ok(())
}

/// Reverse v55→v56. Local and derived: nothing is lost that `stats` cannot
/// compute again.
pub(super) fn down_56_to_55_stats_cache(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "DROP TRIGGER IF EXISTS stats_cache_insert;
         DROP TRIGGER IF EXISTS stats_cache_update;
         DROP TRIGGER IF EXISTS stats_cache_delete;
         DROP INDEX IF EXISTS entries_zoned;
         DROP TABLE IF EXISTS stats_cache;
         DROP TABLE IF EXISTS stats_cache_days;
         DROP TABLE IF EXISTS stats_cache_through;
         DROP TABLE IF EXISTS stats_cache_dirty;
         DELETE FROM _meta WHERE key = 'stats_cache';",
    )?;
    Ok(())
}

/// da-3zqyv: `sync_peers.datum` and `schema`, what a member last said it
/// runs, so a copy behind it can say so.
pub(super) fn m56_to_57_sync_peer_version(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "sync_peers")? {
        return Ok(());
    }
    for (column, ty) in [("datum", "TEXT"), ("schema", "INTEGER")] {
        if !has_column(conn, "sync_peers", column)? {
            conn.execute_batch(&format!("ALTER TABLE sync_peers ADD COLUMN {column} {ty};"))?;
        }
    }
    Ok(())
}

/// Reverse v56→v57. Local: the next handshake says it again.
pub(super) fn down_57_to_56_sync_peer_version(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "sync_peers")? {
        return Ok(());
    }
    conn.execute_batch(
        "ALTER TABLE sync_peers DROP COLUMN datum;
         ALTER TABLE sync_peers DROP COLUMN schema;",
    )?;
    Ok(())
}
