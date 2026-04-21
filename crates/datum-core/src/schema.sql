-- datum SQLite schema
-- All times stored as ISO 8601 UTC strings for portability.
-- CRDT-friendliness: entries are append-mostly; updates use modified_at for LWW.

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS entries (
    slug            TEXT NOT NULL PRIMARY KEY,   -- datum-style "field:time" (user-facing id)
    hid             TEXT NOT NULL,               -- random 12-char base36 [0-9a-z] (prefix-addressable)
    content_hash    TEXT NOT NULL,               -- SHA-256 for dedup (not UNIQUE: Rekey strategy permits duplicates)

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
    device_id       TEXT NOT NULL,

    -- Soft-delete tombstone. NULL = not deleted.
    deleted_at      TEXT,

    -- Template metadata (datum meta.idStructure / meta.fieldStructure).
    -- NULL = slug was built by the default `field:time` rule.
    id_structure    TEXT,
    field_structure TEXT
);

-- Unique hid — collisions on 12-char base36 (~4.7 quintillion space) are vanishingly
-- rare, but the index enforces it and makes prefix lookups on hid fast.
CREATE UNIQUE INDEX IF NOT EXISTS idx_entries_hid ON entries (hid);

-- Efficient queries: by field + time
CREATE INDEX IF NOT EXISTS idx_entries_field
    ON entries (field, COALESCE(time_start_utc, time_end_utc, created_at));

-- Range queries across all fields
CREATE INDEX IF NOT EXISTS idx_entries_time
    ON entries (COALESCE(time_start_utc, time_end_utc, created_at));

-- Dedup lookup.
CREATE INDEX IF NOT EXISTS idx_entries_content_hash
    ON entries (content_hash);

-- Field specs: single source of truth for a field's schema, tracking state,
-- colors, allowed states, id template, and derived aliases. Stored as a JSON
-- blob so the shape can evolve without migrations.
CREATE TABLE IF NOT EXISTS field_specs (
    field       TEXT NOT NULL PRIMARY KEY,
    spec_json   TEXT NOT NULL
);

-- Aliases: user-created and spec-derived. `derived_from_field` is non-NULL
-- when the alias was auto-generated from a FieldSpec state; such rows
-- cascade-delete when the owning spec is deleted or its states change.
CREATE TABLE IF NOT EXISTS aliases (
    name                TEXT NOT NULL PRIMARY KEY,
    alias_json          TEXT NOT NULL,
    derived_from_field  TEXT
);

CREATE INDEX IF NOT EXISTS idx_aliases_derived_from_field
    ON aliases (derived_from_field);

-- Sync state: per-DB bookkeeping for push/pull against a relay. `db_id`
-- identifies the local DB's sync identity (usually the device's sync
-- identity, "local" by default). `push_seq` tracks the highest local
-- modified_at epoch-ms packed into a pushed changeset; `pull_seq` tracks
-- the highest server-assigned global seq consumed from a pull. Timestamps
-- are stored as epoch-ms INTEGER so the relay and clients can round-trip
-- them without format negotiation.
CREATE TABLE IF NOT EXISTS sync_state (
    db_id         TEXT NOT NULL,
    push_seq      INTEGER NOT NULL DEFAULT 0,
    pull_seq      INTEGER NOT NULL DEFAULT 0,
    last_push_at  INTEGER,
    last_pull_at  INTEGER,
    PRIMARY KEY (db_id)
);

-- Schema version and other key/value metadata. `schema_version` is the
-- single source of truth consulted by `datum setup` and Db::open().
CREATE TABLE IF NOT EXISTS _meta (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

-- Seed schema_version at the initial shape. Migrations bump this through
-- the migration runner; the INSERT OR IGNORE keeps existing rows untouched.
INSERT OR IGNORE INTO _meta (key, value) VALUES ('schema_version', '2');
