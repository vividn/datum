//! Migration bodies for schema versions 30 → 40 — the current era.
//!
//! New migrations land here. When `CURRENT_SCHEMA_VERSION` crosses 40,
//! start `v41_v50.rs` rather than growing this file: the era boundary is
//! what keeps any one file from becoming the next 3,500-line monolith.

use rusqlite::{params, Connection, OptionalExtension};

use super::support::*;
use crate::error::Result;

/// Rewrite the stored `CREATE TABLE` SQL for `table_name`, swapping the
/// `raw_input` column definition. Matches the trailing comma so the
/// replacement is anchored: `raw_input <from>,` → `raw_input <to>,`.
///
/// Idempotent: no-op when the target form is already present. The column
/// padding is whatever the table's author wrote — `entries` aligns its
/// types at 7 spaces, `entry_history` at 9 — so the clause is matched
/// past a run of whitespace rather than against an enumerated list of
/// paddings. An earlier version enumerated two, silently missed
/// `entry_history`, and left that column NOT NULL on every migrated
/// database while fresh ones (laid down from schema.sql) had it
/// nullable; the whole-chain shape test in `tests/migration_chain.rs`
/// is what surfaced the divergence.
fn rewrite_raw_input_column(
    conn: &Connection,
    table_name: &str,
    from: &str,
    to: &str,
) -> Result<()> {
    // Some migration tests stand up a partial schema (just `field_specs`,
    // say) and expect every later forward migration to no-op rather than
    // hard-fail on a missing table. Mirror that contract.
    let current_sql: Option<String> = conn
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type='table' AND name=?1",
            [table_name],
            |row| row.get(0),
        )
        .optional()?;
    let Some(current_sql) = current_sql else {
        return Ok(());
    };
    let Some(new_sql) = swap_column_type(&current_sql, "raw_input", from, to) else {
        return Ok(());
    };
    // SQLite caches the parsed schema in the connection. Rewriting
    // `sqlite_schema.sql` alone doesn't bump the cache; subsequent
    // INSERTs on the same connection would still enforce the OLD
    // constraint. Bumping `schema_version` forces a reparse on the next
    // SQL statement, which is the documented escape hatch for the
    // writable_schema editing pattern.
    let schema_version: i64 = conn.query_row("PRAGMA schema_version", [], |row| row.get(0))?;
    conn.execute_batch("PRAGMA writable_schema = 1;")?;
    conn.execute(
        "UPDATE sqlite_schema SET sql = ?1 WHERE type='table' AND name=?2",
        rusqlite::params![new_sql, table_name],
    )?;
    conn.execute_batch(&format!("PRAGMA schema_version = {};", schema_version + 1))?;
    conn.execute_batch("PRAGMA writable_schema = 0;")?;
    Ok(())
}

/// Swap the declared type of `column` in a stored `CREATE TABLE` statement,
/// preserving whatever padding the original used.
///
/// Matches `<column><whitespace><from>,` and requires the trailing comma so
/// that `from = "TEXT"` cannot match the `TEXT` prefix of a longer clause
/// like `TEXT NOT NULL`. Returns `None` when the column is absent or is
/// already in some other shape — the caller treats that as a no-op, which
/// is what makes the migration idempotent.
fn swap_column_type(sql: &str, column: &str, from: &str, to: &str) -> Option<String> {
    let mut cursor = 0;
    while let Some(offset) = sql[cursor..].find(column) {
        let name_end = cursor + offset + column.len();
        cursor = name_end;
        let rest = &sql[name_end..];
        let gap = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        // No whitespace after the name means this is a longer identifier
        // or a mention inside a comment, not the column clause.
        if gap == 0 {
            continue;
        }
        let Some(after) = rest[gap..].strip_prefix(from) else {
            continue;
        };
        if !after.starts_with(',') {
            continue;
        }
        let mut out = String::with_capacity(sql.len() + to.len());
        out.push_str(&sql[..name_end + gap]);
        out.push_str(to);
        out.push_str(after);
        return Some(out);
    }
    None
}

/// Walk `entries` (tombstones included) and rewrite each row's
/// `content_hash` to whatever `hash_of` returns. Keyed on `hid`, the one
/// identity that survives an edit. Idempotent, and a no-op on a partial
/// schema that has no `entries` table — some migration tests stand up only
/// a fragment and expect later forward steps to skip rather than fail.
fn rehash_all_entries(
    conn: &Connection,
    hash_of: impl Fn(&crate::entry::Entry) -> String,
) -> Result<()> {
    let has_entries: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name='entries'",
            [],
            |row| row.get::<_, i64>(0).map(|n| n > 0),
        )
        .unwrap_or(false);
    if !has_entries {
        return Ok(());
    }
    // The live row mapper reads *today's* column names, but this helper
    // runs mid-walk, against whatever shape the schema has at that step.
    // da-b8rs renamed the two uncertainty columns at v34, so a v32 DB
    // being upgraded still calls them `*_precision` here. Alias them
    // forward for the read; any future rename of a mapped column has to
    // extend this list or the rehash breaks on real upgrades.
    let mut select = String::from("SELECT *");
    for (old, new) in [
        ("occur_time_precision", "occur_time_uncertainty"),
        ("dur_precision", "dur_uncertainty"),
    ] {
        if !has_column(conn, "entries", new)? && has_column(conn, "entries", old)? {
            select.push_str(&format!(", {old} AS {new}"));
        }
    }
    select.push_str(" FROM entries");

    let rows: Vec<(String, String)> = {
        let mut stmt = conn.prepare(&select)?;
        let mapped = stmt.query_map([], |row| {
            let entry = crate::db::row_to_entry_public(row)?;
            Ok((entry.hid.to_string(), hash_of(&entry)))
        })?;
        mapped.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut update = conn.prepare("UPDATE entries SET content_hash = ?2 WHERE hid = ?1")?;
    for (hid, hash) in rows {
        update.execute(params![hid, hash])?;
    }
    Ok(())
}

/// da-xuiw: rename `sync_state`'s two muddy "push seq" columns for clarity.
/// `push_seq` (the `modified_at` packing watermark) -> `push_mark`, and
/// `my_push_seq` (the per-device AAD push counter) -> `device_seq`. Pure
/// rename, no data change; `pull_seq` already reads cleanly and is left as
/// is. Guarded per-column so a re-run — or a fresh DB already on the new
/// names — is a no-op.
pub(super) fn m30_to_31_sync_state_rename_seq_columns(conn: &Connection) -> Result<()> {
    if has_column(conn, "sync_state", "push_seq")? {
        conn.execute_batch("ALTER TABLE sync_state RENAME COLUMN push_seq TO push_mark;")?;
    }
    if has_column(conn, "sync_state", "my_push_seq")? {
        conn.execute_batch("ALTER TABLE sync_state RENAME COLUMN my_push_seq TO device_seq;")?;
    }
    Ok(())
}

/// polecat §5 #11: `entries.raw_input` was declared `TEXT NOT NULL
/// DEFAULT ''`, with the contract pretending raw_input would always be
/// non-empty even though some write paths (legacy datum CouchDB import)
/// supply an empty string because the source never captured an argv.
/// Drop the NOT NULL and the DEFAULT — the column is genuinely
/// nullable now, with the empty-string sentinel maintained only on the
/// read side (the row mapper normalises empty → NULL) for backwards
/// compatibility with already-stored rows.
///
/// Implementation: SQLite's ALTER TABLE can't toggle a column's
/// NULL-ability directly, but it CAN parse a hand-edited
/// `sqlite_schema.sql`. We flip `writable_schema = 1`, rewrite the
/// `entries` table's stored CREATE TABLE string with the constraint
/// removed, then flip it back off. SQLite rebuilds its in-memory
/// schema on the next open; existing row data is unaffected.
///
/// The trick is the recommended path in the SQLite docs ("Making Other
/// Kinds Of Table Schema Changes") for the specific case of dropping
/// NOT NULL — a full table rebuild would be 2-3× the I/O for the same
/// outcome on a column that already only stores valid (non-NULL) text.
pub(super) fn m31_to_32_raw_input_nullable(conn: &Connection) -> Result<()> {
    rewrite_raw_input_column(conn, "entries", "TEXT NOT NULL DEFAULT ''", "TEXT")?;
    rewrite_raw_input_column(conn, "entry_history", "TEXT NOT NULL", "TEXT")?;
    Ok(())
}

/// da-4vyw: `content_hash` now covers `following_state` and deleted-ness on
/// top of the original `field · occur_time · dur · state · data`. Every
/// stored row's hash therefore has to be rewritten, or the whole database
/// reads as drifted to `datum check` and dedup compares old hashes against
/// new ones.
///
/// SHA-256 is not available to SQLite here, so this walks the rows in Rust
/// rather than running one `UPDATE`. It deliberately materializes each row
/// through the live row mapper and asks [`Entry::expected_content_hash`]
/// for the value: that is the exact predicate `datum check` uses, so the
/// migration cannot leave behind a row the checker would then flag. The
/// cost of that choice is that this migration is coupled to the current
/// hash definition rather than frozen at v33 — which is harmless, because
/// the *next* hash change must ship its own full rewrite anyway.
///
/// Structurally a no-op (an in-place UPDATE, like m21→m22), so
/// [`infer_legacy_version`] has nothing to probe for.
pub(super) fn m32_to_33_content_hash_v33(conn: &Connection) -> Result<()> {
    rehash_all_entries(conn, |entry| entry.expected_content_hash())
}

/// da-b8rs: `precision` and `uncertainty` were two names for one thing,
/// split across a crate boundary, which is why nobody could tell whether
/// uncertainty reached storage. It did — under the other name. Rename both
/// columns so the store says what the code says.
///
/// Pure rename: no data is read, rewritten or dropped, and the values keep
/// their meaning (an ISO-8601 ± radius, NULL = unknown). Guarded per column
/// so a re-run — or a fresh DB already laid down from `schema.sql` — is a
/// no-op.
///
/// The risk here is not SQL, it is rollout: `Entry` crosses the sync wire as
/// serde JSON, no struct sets `deny_unknown_fields`, and so a peer that does
/// not know a field silently drops it instead of erroring. Upgrade every
/// device before any device writes the new shape; `ops::parse_payload` reads
/// the old names for the transition window.
pub(super) fn m33_to_34_precision_to_uncertainty(conn: &Connection) -> Result<()> {
    if has_column(conn, "entries", "occur_time_precision")? {
        conn.execute_batch(
            "ALTER TABLE entries RENAME COLUMN occur_time_precision TO occur_time_uncertainty;",
        )?;
    }
    if has_column(conn, "entries", "dur_precision")? {
        conn.execute_batch("ALTER TABLE entries RENAME COLUMN dur_precision TO dur_uncertainty;")?;
    }
    Ok(())
}

// ─── Down migrations ─────────────────────────────────────────────────
//
// Each forward migration above has a paired `down` here that reverses
// the schema change. Conventions:
//
// * Every down is wrapped in the same outer transaction the runner
//   opens, so a partial failure rolls the whole step back.
// * Pure schema changes (CREATE INDEX, CREATE TABLE) reverse trivially.
// * Column additions are reversed via `ALTER TABLE ... DROP COLUMN`
//   (SQLite ≥ 3.35) on tables that have no generated columns. For
//   `entries` we still use DROP COLUMN because the dropped columns
//   (`extra_metadata`, `user_asserted_last_state`) are not referenced
//   by the generated `verb` / `dur_seconds` projections, so the drop
//   is safe.
// * Forward steps that destructively rewrote a table or wrote new rows
//   (v2→v3, v8→v9, v11→v12) document the data-loss in the down's
//   docstring. The down still runs to make the schema reversible for
//   round-trip testing; production callers should back up before
//   walking back across these.

/// Reverse 34→33 — restore the pre-da-b8rs column names. Lossless.
pub(super) fn down_34_to_33_precision_to_uncertainty(conn: &Connection) -> Result<()> {
    if has_column(conn, "entries", "occur_time_uncertainty")? {
        conn.execute_batch(
            "ALTER TABLE entries RENAME COLUMN occur_time_uncertainty TO occur_time_precision;",
        )?;
    }
    if has_column(conn, "entries", "dur_uncertainty")? {
        conn.execute_batch("ALTER TABLE entries RENAME COLUMN dur_uncertainty TO dur_precision;")?;
    }
    Ok(())
}

/// Reverse 33→32 — recompute every hash under the pre-da-4vyw definition
/// (`field · occur_time · dur · state · data`, `\x00`-separated), which is
/// reproduced locally here because the live one has moved on.
pub(super) fn down_33_to_32_content_hash_v33(conn: &Connection) -> Result<()> {
    rehash_all_entries(conn, |entry| {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        for (i, part) in [
            entry.field.clone(),
            entry
                .occur_time
                .as_ref()
                .map(|t| t.utc.to_rfc3339())
                .unwrap_or_default(),
            entry
                .dur
                .as_ref()
                .map(crate::dur::Dur::to_storage_string)
                .unwrap_or_default(),
            entry
                .state
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_default(),
            entry
                .data
                .as_ref()
                .map(|v| v.to_string())
                .unwrap_or_default(),
        ]
        .iter()
        .enumerate()
        {
            if i > 0 {
                h.update(b"\x00");
            }
            h.update(part.as_bytes());
        }
        format!("{:x}", h.finalize())
    })
}

/// Reverse 32→31 — restore the `NOT NULL DEFAULT ''` constraint via the
/// same writable_schema mechanism. Existing NULL rows would fail the
/// reapplied constraint, so we UPDATE them to the empty-string
/// sentinel first; the data-loss is minimal (NULL ↔ '' is the
/// transparent mapping the row mapper preserves anyway).
pub(super) fn down_32_to_31_raw_input_nullable(conn: &Connection) -> Result<()> {
    conn.execute(
        "UPDATE entries SET raw_input = '' WHERE raw_input IS NULL",
        [],
    )?;
    conn.execute(
        "UPDATE entry_history SET raw_input = '' WHERE raw_input IS NULL",
        [],
    )?;
    rewrite_raw_input_column(conn, "entries", "TEXT", "TEXT NOT NULL DEFAULT ''")?;
    rewrite_raw_input_column(conn, "entry_history", "TEXT", "TEXT NOT NULL")?;
    Ok(())
}

/// Reverse 31→30 — restore the pre-da-xuiw column names.
pub(super) fn down_31_to_30_sync_state_rename_seq_columns(conn: &Connection) -> Result<()> {
    if has_column(conn, "sync_state", "push_mark")? {
        conn.execute_batch("ALTER TABLE sync_state RENAME COLUMN push_mark TO push_seq;")?;
    }
    if has_column(conn, "sync_state", "device_seq")? {
        conn.execute_batch("ALTER TABLE sync_state RENAME COLUMN device_seq TO my_push_seq;")?;
    }
    Ok(())
}
