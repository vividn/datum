-- datum SQLite schema (v3 — data model v2)
--
-- Timestamps are stored as ISO 8601 UTC strings. CRDT-friendliness: entries
-- are append-mostly; updates use modify_time for LWW. hid is the primary key
-- (random base36, prefix-addressable); slug is a human-facing unique key
-- derived at write time from field + id-keys (or timestamp fallback).

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS entries (
    -- Identity
    hid             TEXT NOT NULL PRIMARY KEY,   -- random 12-char base36, prefix-addressable
    slug            TEXT NOT NULL,               -- datum-style human reference, UNIQUE indexed below
    content_hash    TEXT NOT NULL,               -- SHA-256 for dedup (not UNIQUE: Rekey permits dupes)

    -- What
    field           TEXT NOT NULL,
    entry_shape     TEXT NOT NULL,               -- 'point' | 'block' | 'ongoing'
    intent          TEXT,                        -- NULL | 'stop' | 'add' | 'occur' | 'switch' (user-intent override for generated verb)
    value           TEXT,                        -- JSON (primary value)
    data            TEXT,                        -- JSON (nested, may use {v,u} unit annotations)
    unit            TEXT,                        -- top-level unit for value

    -- When
    start_time      TEXT,                        -- ISO 8601 UTC
    start_time_offset INTEGER,                   -- UTC offset in minutes
    start_time_tz   TEXT,                        -- IANA timezone name
    start_time_precision INTEGER NOT NULL DEFAULT 0,     -- absolute uncertainty in seconds
    start_time_precision_unit TEXT NOT NULL DEFAULT 'exact', -- exact|second|minute|hour|date|month|year
    end_time        TEXT,
    end_time_offset INTEGER,
    end_time_tz     TEXT,
    end_time_precision INTEGER NOT NULL DEFAULT 0,
    end_time_precision_unit TEXT NOT NULL DEFAULT 'exact',

    -- Where
    lat             REAL,
    lon             REAL,
    location        TEXT,

    -- Bookkeeping
    create_time     TEXT NOT NULL,
    modify_time     TEXT NOT NULL,
    device_id       TEXT NOT NULL,

    -- Soft-delete tombstone. NULL = not deleted.
    deleted_at      TEXT,

    -- Template metadata (mirrors datum meta.idStructure / meta.fieldStructure).
    id_structure    TEXT,
    field_structure TEXT,

    -- Intent capture: record the literal user input so alias/parser changes
    -- can be re-derived without losing source truth.
    raw_input       TEXT NOT NULL DEFAULT '',
    alias_chain     TEXT,                        -- JSON array of alias expansions, nullable
    parser_version  INTEGER NOT NULL DEFAULT 1,

    -- Generated columns — read-only projections.
    --
    -- `verb`: intent overrides shape-derived classification, so `stop`/`note`
    -- survive as first-class verbs for state compilation and the CLI's verb
    -- filters. When intent is NULL, shape + value determine the verb.
    verb TEXT GENERATED ALWAYS AS (
        CASE
            WHEN intent IS NOT NULL THEN intent
            WHEN entry_shape = 'ongoing' THEN 'start'
            WHEN entry_shape = 'block' THEN 'block'
            WHEN entry_shape = 'point' AND value IS NULL THEN 'occur'
            WHEN entry_shape = 'point' AND json_valid(value)
                 AND json_type(value) IN ('true','false') THEN 'switch'
            WHEN entry_shape = 'point' AND value IS NOT NULL THEN 'switch'
            ELSE 'add'
        END
    ) VIRTUAL,

    -- `dur_seconds`: derived duration in integer seconds. NULL when either
    -- bound is absent, so callers can coalesce without a separate check.
    dur_seconds INTEGER GENERATED ALWAYS AS (
        CASE WHEN start_time IS NOT NULL AND end_time IS NOT NULL
            THEN CAST((julianday(end_time) - julianday(start_time)) * 86400 AS INTEGER)
            ELSE NULL
        END
    ) VIRTUAL
);

-- Unique slug — the human-facing id. Prevents accidental slug collisions on
-- write and makes slug→hid lookups single-hop.
CREATE UNIQUE INDEX IF NOT EXISTS entries_slug ON entries (slug);

-- Field-scoped tail queries: most reads are "last N entries for field F" so
-- a composite (field, start_time DESC) index lets them avoid a sort.
CREATE INDEX IF NOT EXISTS entries_field_start
    ON entries (field, start_time DESC);

-- Live-only variant used by the SQL-backed views (R3). The views consistently
-- filter tombstones and timestamped entries per field; a partial index lets
-- SQLite skip deleted rows entirely rather than filter them after an index
-- lookup.
CREATE INDEX IF NOT EXISTS entries_live_field_start
    ON entries (field, start_time)
    WHERE deleted_at IS NULL AND start_time IS NOT NULL;

-- Dedup lookup on the hashed content fingerprint.
CREATE INDEX IF NOT EXISTS entries_content_hash
    ON entries (content_hash);

-- Filter by derived verb (e.g. "only switch events in this window").
CREATE INDEX IF NOT EXISTS entries_verb ON entries (verb);

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

-- Seed alias: `note` routes to `add`. After D3 `note` is no longer a
-- first-class verb; this alias preserves muscle memory. User can override
-- or remove via `datum alias set note ...` or `datum alias remove note`.
INSERT OR IGNORE INTO aliases (name, alias_json, derived_from_field)
VALUES (
    'note',
    '{"name":"note","expands_to":"add","description":"compat: note → add (seeded)"}',
    NULL
);

-- Sync state: per-DB bookkeeping for push/pull against a relay.
CREATE TABLE IF NOT EXISTS sync_state (
    db_id         TEXT NOT NULL,
    push_seq      INTEGER NOT NULL DEFAULT 0,
    pull_seq      INTEGER NOT NULL DEFAULT 0,
    last_push_at  INTEGER,
    last_pull_at  INTEGER,
    PRIMARY KEY (db_id)
);

-- Per-entry modification trail. Append-only. Every mutating operation on the
-- entries table writes a row here in the same transaction. `snapshot` holds
-- the full entry JSON: the newly-created state for version=1 (change_kind
-- 'create'), otherwise the PRE-operation state so a revert can restore it.
CREATE TABLE IF NOT EXISTS entry_history (
    entry_hid         TEXT NOT NULL,
    version           INTEGER NOT NULL,
    snapshot          TEXT NOT NULL,
    modify_time       TEXT NOT NULL,
    raw_input         TEXT NOT NULL,
    alias_chain       TEXT,
    device_id         TEXT NOT NULL,
    change_kind       TEXT NOT NULL,
    PRIMARY KEY (entry_hid, version)
);

CREATE INDEX IF NOT EXISTS entry_history_entry
    ON entry_history(entry_hid, version DESC);

CREATE INDEX IF NOT EXISTS entry_history_change_kind
    ON entry_history(change_kind);

-- Overlap / double-open / orphan-end bookkeeping. Rows are created by the
-- local validator (D5) on insert and by sync-pull reconciliation (C4e);
-- the `detected_by` column distinguishes them. Rows start with
-- `resolution=NULL` and are filled in by `datum conflicts resolve` or the
-- auto-resolver once they're old enough.
--
-- `id` is a random 12-char base36 token (full precision); `hid` is the
-- short prefix used at the CLI. `entry_slug` is the slug of the newer
-- triggering entry. `local_entry` holds that triggering entry as JSON;
-- `remote_entry` holds the conflicting counterpart — the older local entry
-- for `local_validator` rows, the incoming remote entry for `sync_pull`
-- rows. May be NULL for orphan_end detection (no counterpart).
CREATE TABLE IF NOT EXISTS sync_conflicts (
    id              TEXT NOT NULL PRIMARY KEY,
    hid             TEXT NOT NULL,
    entry_slug      TEXT NOT NULL,
    local_entry     TEXT NOT NULL,
    remote_entry    TEXT,
    detected_at     TEXT NOT NULL,
    resolution      TEXT,
    resolved_at     TEXT,
    detected_by     TEXT NOT NULL,
    conflict_kind   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS sync_conflicts_hid ON sync_conflicts(hid);
CREATE INDEX IF NOT EXISTS sync_conflicts_slug ON sync_conflicts(entry_slug);
CREATE INDEX IF NOT EXISTS sync_conflicts_pending
    ON sync_conflicts(detected_at) WHERE resolution IS NULL;

-- Schema version and other key/value metadata.
CREATE TABLE IF NOT EXISTS _meta (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);

-- Seed schema_version at the initial shape. Migrations bump this through
-- the migration runner; the INSERT OR IGNORE keeps existing rows untouched.
INSERT OR IGNORE INTO _meta (key, value) VALUES ('schema_version', '6');
