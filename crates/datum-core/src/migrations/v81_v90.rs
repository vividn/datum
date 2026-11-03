//! Migration bodies for schema versions 80 → 90 — the current era.
//!
//! New migrations land here. When `CURRENT_SCHEMA_VERSION` crosses 90,
//! start `v91_v100.rs` rather than growing this file.

use rusqlite::Connection;

use super::support::{has_column, table_exists};

use crate::error::Result;

/// da-52vdj: the sync floor each peer said in its handshake, beside its
/// schema, so a copy says which of two across a floor must update. Local.
pub(super) fn m80_to_81_peer_floor(conn: &Connection) -> Result<()> {
    if table_exists(conn, "sync_peers")? && !has_column(conn, "sync_peers", "floor")? {
        conn.execute_batch("ALTER TABLE sync_peers ADD COLUMN floor INTEGER;")?;
    }
    Ok(())
}

/// Reverse v80→v81: the floors go, read again at each peer's next handshake.
pub(super) fn down_81_to_80_peer_floor(conn: &Connection) -> Result<()> {
    if table_exists(conn, "sync_peers")? && has_column(conn, "sync_peers", "floor")? {
        conn.execute_batch("ALTER TABLE sync_peers DROP COLUMN floor;")?;
    }
    Ok(())
}
