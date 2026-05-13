-- datum SQLite schema (v25 — partial UNIQUE on (field, occur_time), da-ga8b).
--
-- Time-shape pivot 2026-05-01: the entries table reverts from the
-- start_time/end_time/entry_shape/intent shape to the original datum
-- model — `occur_time` (when), `state` (what), `dur` (how long, ISO
-- duration string), `following_state` (what changed to). The verb is
-- derived from the (state, dur, following_state) triple but stored as
-- a plain column so it can be indexed and `dm check verbs` can detect
-- drift.
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
    -- `state`: any JSON value; NULL means undefined (intentionally
    -- different from original datum, which collapsed null and
    -- undefined). Promoted to literal `true` at insert when paired with
    -- a switch/block-switch shape so the verb-derivation table is total.
    state           TEXT,
    -- `dur`: ISO-8601 duration string (e.g. "PT30M", "P1D"). NULL = no
    -- duration (point/switch). "PT0S" = explicit zero (point-with-state
    -- or point-switch). Negative durations are not supported.
    dur             TEXT,
    -- `following_state`: target state for switch-style events. Only
    -- meaningful when `dur` is set; the table-level CHECK below enforces
    -- this. Same JSON shape as `state`.
    following_state TEXT,
    -- `verb`: stored-not-derived classifier. One of point | switch |
    -- block | block-switch | point-switch. Derived from the (state, dur,
    -- following_state) triple at insert/update time; `dm check verbs`
    -- detects drift between stored and re-derived values.
    verb            TEXT NOT NULL DEFAULT 'point',
    data            TEXT,                        -- JSON (nested, may use {v,u} unit annotations)

    -- When
    occur_time           TEXT NOT NULL,          -- ISO 8601 UTC, required
    occur_time_offset    INTEGER,                -- UTC offset in minutes
    occur_time_tz        TEXT,                   -- IANA timezone name
    -- Single ISO-8601 duration string (e.g. "PT1S", "PT1M", "PT1H",
    -- "P1D", "P1W"). Replaces the v3-era (precision INTEGER seconds,
    -- precision_unit TEXT) pair.
    occur_time_precision TEXT,
    -- ISO 8601 duration string capturing the granularity at which
    -- `dur` is meaningful (e.g. "PT1M" for minute-resolution). NULL =
    -- unspecified; the renderer falls back to the full `dur` value.
    dur_precision        TEXT,

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

    -- S6: when the user passed `-L/--last-state=<v>`, the normalized assertion
    -- value is stored here (JSON). NULL for the common case (no assertion).
    -- Cross-checked against computed last-state at insert; mismatches file a
    -- `last_state_mismatch` row in sync_conflicts rather than blocking the write.
    user_asserted_last_state TEXT,

    -- da-tsw: system-managed provenance, strictly disjoint from `data` (the
    -- user-supplied JSON bucket). Writers: migrate, sync conflict resolver,
    -- future imports. Readers: dedup-on-reimport (`original_id`), debugging.
    -- Hidden from default render paths (tail/now/show) — only `-A`/Show::All
    -- exposes it. SURVIVES `datum edit` of `data`; only re-imports / sync
    -- conflict resolution mutate it. Not part of `content_hash`.
    extra_metadata TEXT,

    -- `following_state` is only meaningful when `dur` is set. The
    -- parser-level rejection in da-5ns.3 keeps malformed inserts from
    -- ever reaching this layer; the CHECK is a defence-in-depth gate
    -- against accidental writes from future code paths.
    CHECK (following_state IS NULL OR dur IS NOT NULL)
);

-- Unique slug — the human-facing id. Prevents accidental slug collisions on
-- write and makes slug→hid lookups single-hop.
CREATE UNIQUE INDEX IF NOT EXISTS entries_slug ON entries (slug);

-- da-ga8b: partial UNIQUE on (field, occur_time). Slug uniqueness catches
-- the default case where occur_time appears in the slug template, but
-- custom ids (`-K`, `--id`, or `spec.id_template`) can produce distinct
-- slugs that still share the same (field, occur_time) — semantically two
-- state-changes for the same field at the same exact instant, which is
-- undefined. The partial filter excludes tombstones so the soft-delete +
-- re-insert pattern still works.
CREATE UNIQUE INDEX IF NOT EXISTS entries_field_occurtime
    ON entries (field, occur_time)
    WHERE occur_time IS NOT NULL AND deleted_at IS NULL;

-- Field-scoped tail queries: most reads are "last N entries for field F" so
-- a composite (field, occur_time DESC) index lets them avoid a sort.
CREATE INDEX IF NOT EXISTS entries_field_occur
    ON entries (field, occur_time DESC);

-- Live-only variant used by the SQL-backed views (R3). The views consistently
-- filter tombstones per field; a partial index lets SQLite skip deleted rows
-- entirely rather than filter them after an index lookup. `occur_time` is
-- NOT NULL so no separate `IS NOT NULL` predicate is needed.
CREATE INDEX IF NOT EXISTS entries_live_field_occur
    ON entries (field, occur_time)
    WHERE deleted_at IS NULL;

-- Dedup lookup on the hashed content fingerprint.
CREATE INDEX IF NOT EXISTS entries_content_hash
    ON entries (content_hash);

-- Filter by stored verb (e.g. "only switch events in this window").
CREATE INDEX IF NOT EXISTS entries_verb ON entries (verb);

-- Bare-time tail/head (no `--field` filter) orders by `occur_time`.
-- Without this index the planner falls back to a full SCAN + temp
-- B-tree sort, which dominates at 100k+ entries (see da-7iu). Including
-- `slug` matches the secondary tie-break so the index covers the full
-- ORDER BY. Partial-on-live keeps it skinny.
CREATE INDEX IF NOT EXISTS entries_occur_time
    ON entries (occur_time, slug)
    WHERE deleted_at IS NULL;

-- Drift-detection partial indexes (da-b423). Back `dm check slugs` scans
-- that need to walk just the rows with a non-NULL template. Selectivity
-- is asymmetric: almost every row carries `id_structure` (default
-- `%occur_time%` / `%create_time%` template), while `field_structure` is
-- set only when the user writes a templated field like
-- `word__%srcLang%`. Splitting into two partial indexes lets the planner
-- pick the tiny one for `field_structure` scans without paying the cost
-- of the near-full-table version.
CREATE INDEX IF NOT EXISTS entries_id_structure
    ON entries (hid) WHERE id_structure IS NOT NULL;
CREATE INDEX IF NOT EXISTS entries_field_structure
    ON entries (hid) WHERE field_structure IS NOT NULL;

-- Field specs: single source of truth for a field's schema, tracking state,
-- colors, allowed states, id template, and derived aliases. Stored as a JSON
-- blob so the shape can evolve without migrations. `kit_id` is NULL for
-- user-declared specs and the kit's name for rows installed via
-- `datum kit install`; uninstall removes only rows with that kit_id.
CREATE TABLE IF NOT EXISTS field_specs (
    field       TEXT NOT NULL PRIMARY KEY,
    spec_json   TEXT NOT NULL,
    kit_id      TEXT,
    -- Mirror of `FieldSpec.tags` (JSON array of strings, default '[]').
    -- Pulled out of `spec_json` so SQL filters (`tags LIKE '%"chore"%'`) and
    -- the `datum spec list --tag` query can avoid JSON parsing.
    tags        TEXT NOT NULL DEFAULT '[]',
    -- Mirror of `FieldSpec.render` (JSON: bool or {surface: bool} map).
    -- NULL means the implicit default — visible on every surface. Pulled
    -- out of `spec_json` so future per-surface visibility filters can run
    -- without parsing the full spec body.
    render_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_field_specs_kit_id ON field_specs(kit_id);

-- Named views: read-only SELECT statements installed by kits or declared
-- by the user. `kit_id` is NULL for user-declared rows, the kit's name
-- for rows installed via `datum kit install`. Powers `datum view <name>`.
-- Read-only enforcement happens at execution time (PRAGMA query_only=1);
-- this table just stores the SQL text.
CREATE TABLE IF NOT EXISTS views (
    name      TEXT NOT NULL PRIMARY KEY,
    sql_text  TEXT NOT NULL,
    kit_id    TEXT
);

CREATE INDEX IF NOT EXISTS idx_views_kit_id ON views(kit_id);

-- Kit-installed indices on `entries` (slice 5). Each row tracks one DDL
-- statement created by `datum kit install`; uninstall does
-- `DROP INDEX IF EXISTS <on_disk_name>` for every row matching `kit_id`
-- and then deletes the rows. `sql_text` is stored verbatim so reinstalls
-- can detect "no-op" cases and so a DB inspector can see what was
-- actually run. Re-install drops every row tagged with the kit and
-- rewrites them, so identity is `(kit_id, index_name)`.
CREATE TABLE IF NOT EXISTS kit_indices (
    kit_id      TEXT NOT NULL,
    index_name  TEXT NOT NULL,
    on_disk_name TEXT NOT NULL,
    sql_text    TEXT NOT NULL,
    PRIMARY KEY (kit_id, index_name)
);

-- Kit-defined commands (slice 6). Two flavours, distinguished by which
-- payload column is non-NULL:
--   - Level 1 (alias-style): `expands_to` is set; the dispatcher splices the
--     stored token list before the parser runs.
--   - Level 2 (view-rendered): `view_name` is set; the dispatcher invokes a
--     read-only renderer that runs the named view, projects `columns_json`,
--     applies `labels_json` / `formats_json` / `sort_col` / `color_rules_json`,
--     and prints the result. Level 3 (`script_text`) is parsed and reserved
--     for forward-compat but the renderer errors at execution time.
-- `kit_id` tags the owning kit (NULL for user-declared rows; user-declared
-- rows are not yet wired but the schema permits them for parity with views).
CREATE TABLE IF NOT EXISTS kit_commands (
    name              TEXT NOT NULL PRIMARY KEY,
    kit_id            TEXT,
    expands_to        TEXT,
    view_name         TEXT,
    help              TEXT,
    columns_json      TEXT,
    labels_json       TEXT,
    formats_json      TEXT,
    sort_col          TEXT,
    flags_json        TEXT,
    color_rules_json  TEXT,
    script_text       TEXT
);

CREATE INDEX IF NOT EXISTS idx_kit_commands_kit_id ON kit_commands(kit_id);


-- Aliases: user-created and kit-installed. Spec-derived aliases live in
-- `field_specs.spec_json` and are surfaced lazily via the in-memory
-- SpecAliasIndex (see `db::Db::spec_alias_index`); they never write rows
-- to this table. `kit_id` is non-NULL for rows installed via
-- `datum kit install` from a `[[alias]]` block; uninstall removes only
-- rows with the matching kit_id.
CREATE TABLE IF NOT EXISTS aliases (
    name                TEXT NOT NULL PRIMARY KEY,
    alias_json          TEXT NOT NULL,
    kit_id              TEXT
);

CREATE INDEX IF NOT EXISTS idx_aliases_kit_id
    ON aliases (kit_id);

-- Seed alias: `note` routes to `add`. After D3 `note` is no longer a
-- first-class verb; this alias preserves muscle memory. User can override
-- or remove via `datum alias set note ...` or `datum alias remove note`.
INSERT OR IGNORE INTO aliases (name, alias_json)
VALUES (
    'note',
    '{"name":"note","expands_to":"add","description":"compat: note → add (seeded)"}'
);

-- Sync state: per-DB bookkeeping for push/pull against a relay.
--
-- `push_seq`     — largest `modified_at` epoch-ms included in any prior push
--                  (advances `pack_changeset`'s `since_seq` watermark).
-- `pull_seq`     — largest server-assigned global seq consumed in a pull.
-- `my_push_seq`  — per-device counter matching the relay's server-side
--                  monotonic `seq` assignment; used as the AAD seq when
--                  encrypting outgoing changesets (the client and server
--                  agree because the server uses `MAX(seq)+1` per device).
CREATE TABLE IF NOT EXISTS sync_state (
    db_id         TEXT NOT NULL,
    push_seq      INTEGER NOT NULL DEFAULT 0,
    pull_seq      INTEGER NOT NULL DEFAULT 0,
    my_push_seq   INTEGER NOT NULL DEFAULT 0,
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

-- Singleton pointer to the hids of the most-recently-accessed batch.
-- Backs the `_LAST` / `_LAST_WITH_PROTECTION` quickId tokens. See
-- `last_docs.rs` for read/write semantics.
CREATE TABLE IF NOT EXISTS last_docs (
    id    INTEGER NOT NULL PRIMARY KEY,
    hids  TEXT NOT NULL,
    time  TEXT NOT NULL
);

-- Schema version and other key/value metadata.
--
-- IMPORTANT: schema.sql does NOT seed `schema_version`. The migration
-- runner owns that, via `infer_legacy_version` + `set_schema_version`,
-- after introspecting actual table/column shapes. A literal INSERT here
-- would falsely stamp every pre-`_meta` legacy DB at the seed version
-- (the 2026-04-30 kit_id incident — schema.sql ran INSERT OR IGNORE,
-- read_schema_version then returned the seed, and migrations v1..v17
-- — including m12_to_13 — were skipped against a still-v12-shape DB).
CREATE TABLE IF NOT EXISTS _meta (
    key    TEXT PRIMARY KEY,
    value  TEXT NOT NULL
);
