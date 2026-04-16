-- datum SQLite schema
-- All times stored as ISO 8601 UTC strings for portability.
-- CRDT-friendliness: entries are append-mostly; updates use modified_at for LWW.

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS entries (
    id              TEXT NOT NULL PRIMARY KEY,   -- ULID
    slug            TEXT NOT NULL,               -- field:timestamp (human ref)
    content_hash    TEXT NOT NULL UNIQUE,        -- SHA-256 for dedup

    -- What
    field           TEXT NOT NULL,
    verb            TEXT NOT NULL,               -- occur|start|stop|switch|note
    value           TEXT,                        -- JSON
    extra_data      TEXT,                        -- JSON (nested, may have {v,u} unit annotations)
    unit            TEXT,                        -- top-level unit for value

    -- When
    time_start_utc  TEXT,                        -- ISO 8601
    time_start_offset INTEGER,                   -- UTC offset in minutes
    time_start_tz   TEXT,                        -- IANA timezone name
    time_end_utc    TEXT,
    time_end_offset INTEGER,
    time_end_tz     TEXT,

    -- Where
    lat             REAL,
    lon             REAL,
    location        TEXT,

    -- Bookkeeping
    created_at      TEXT NOT NULL,
    modified_at     TEXT NOT NULL,
    device_id       TEXT NOT NULL
);

-- Efficient queries: by field + time
CREATE INDEX IF NOT EXISTS idx_entries_field
    ON entries (field, COALESCE(time_start_utc, time_end_utc, created_at));

-- Range queries across all fields
CREATE INDEX IF NOT EXISTS idx_entries_time
    ON entries (COALESCE(time_start_utc, time_end_utc, created_at));

-- Field specs (schemas for known fields)
CREATE TABLE IF NOT EXISTS field_specs (
    field       TEXT NOT NULL PRIMARY KEY,
    spec_json   TEXT NOT NULL
);

-- Aliases
CREATE TABLE IF NOT EXISTS aliases (
    name        TEXT NOT NULL PRIMARY KEY,
    alias_json  TEXT NOT NULL
);

-- Sync state: per-device vector clock for changeset-based sync
CREATE TABLE IF NOT EXISTS sync_state (
    device_id   TEXT NOT NULL PRIMARY KEY,
    last_seq    TEXT NOT NULL,           -- ULID of last synced entry from this device
    updated_at  TEXT NOT NULL
);
