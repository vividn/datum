-- datum SQLite schema (v35 — superseded_by replaces slug-parking as the sync
-- merge's way of keeping a loser without letting it hold a live identity,
-- da-uxgv).
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

-- Connection pragmas (journal_mode, busy_timeout, synchronous) live in
-- Db::configure_connection — set at open, before any migration write,
-- so unmigrated opens are covered too (da-11qq).
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
    data            TEXT,                        -- JSON (nested)

    -- When
    occur_time           TEXT,                   -- ISO 8601 UTC; NULL = timeless (bare `add`), da-lxyw
    occur_time_offset    INTEGER,                -- UTC offset in minutes
    occur_time_tz        TEXT,                   -- IANA timezone name
    -- How precisely `occur_time` is known: a single ISO-8601 duration
    -- string giving the ± radius (e.g. "PT1S", "PT1M", "PT1H", "P1W").
    -- Replaces the v3-era (precision INTEGER seconds, precision_unit
    -- TEXT) pair; named `occur_time_precision` before da-b8rs.
    -- "P1D" is the exception: not a radius but the date-only marker —
    -- the row has a date and no time of day, its instant stripped to
    -- local midnight — and the code branches on it by name, as
    -- `datum_parse::entry_id::DATE_ONLY_PRECISION`.
    -- NULL = unknown, which is NOT the claim "exact" — same rule
    -- as occur_time_tz above.
    occur_time_uncertainty TEXT,
    -- How precisely `dur` is known, same ISO-8601 ± radius (e.g. "PT1M"
    -- for minute-resolution). NULL = unknown; the renderer falls back to
    -- the full `dur` value. Named `dur_precision` before da-b8rs.
    dur_uncertainty      TEXT,

    -- Where
    lat             REAL,
    lon             REAL,
    location        TEXT,

    -- Bookkeeping
    create_time     TEXT NOT NULL,
    modify_time     TEXT NOT NULL,
    client_id       TEXT NOT NULL,

    -- Soft-delete tombstone. NULL = not deleted.
    deleted_at      TEXT,

    -- da-uxgv: the hid of the row that beat this one for an identity the
    -- table can only give to one row (a slug, or a live (field,
    -- occur_time)). NULL — the overwhelmingly common case — means the row
    -- holds its own identity and is in the live stream. Non-NULL means the
    -- row still exists, still carries its own naturally-derived slug and is
    -- still addressable by hid, but is excluded from the live stream and
    -- from both UNIQUE indexes below.
    --
    -- Same shape as `deleted_at`, different reason: `deleted_at` is *the
    -- user deleted this*, `superseded_by` is *a sync merge set this aside*.
    -- Deleting a winner therefore does NOT promote its loser — promotion is
    -- clearing this column, never a side effect.
    --
    -- The pointer is ADVISORY: slug derives from occur_time, so a retime on
    -- either row moves the slug, and hid is the only identity that outlives
    -- an edit — which is why this is an hid and not a bare boolean. It is
    -- local, derived state and does NOT cross the sync wire; every device
    -- reaches the same supersession itself from the rows it holds.
    superseded_by   TEXT,

    -- Template metadata (mirrors datum meta.idStructure / meta.fieldStructure).
    id_structure    TEXT,
    field_structure TEXT,

    -- Intent capture: record the literal user input so alias/parser changes
    -- can be re-derived without losing source truth. NULLable because some
    -- write paths (legacy datum CouchDB import) supply no argv at all;
    -- the row mapper normalises legacy empty-string rows to NULL too so
    -- "no raw_input" has one canonical representation (polecat §5 #11).
    raw_input       TEXT,
    alias_chain     TEXT,                        -- JSON array of alias-expansion and spec_keys steps, nullable
    parser_version  INTEGER NOT NULL DEFAULT 1,

    -- S6: when the user passed `-L/--last-state=<v>`, the normalized assertion
    -- value is stored here (JSON). NULL for the common case (no assertion).
    -- Cross-checked against computed last-state at insert; mismatches file a
    -- `last_state_mismatch` row in integrity_warnings rather than blocking the write.
    user_asserted_last_state TEXT,

    -- da-tsw: system-managed provenance, strictly disjoint from `data` (the
    -- user-supplied JSON bucket). Writers: migrate, sync conflict resolver,
    -- future imports. Readers: dedup-on-reimport (`original_id`), debugging.
    -- Hidden from default render paths (tail/now/show) — only `-A`/Show::All
    -- exposes it. SURVIVES `datum edit` of `data`; only re-imports / sync
    -- conflict resolution mutate it. Not part of `content_hash`.
    extra_metadata TEXT,

    -- da-x3da: epoch-ms of `modify_time`, derived (not stored input) so it
    -- always tracks the RFC3339 source. VIRTUAL + the index below let sync
    -- push filter `WHERE modify_time_ms > ?` instead of scanning every row.
    -- `unixepoch(.., 'subsec')` gives fractional seconds; ×1000 + truncating
    -- CAST yields ms. The push watermark uses this same metric on both ends,
    -- so its sub-ms float deviation from chrono is self-consistent.
    modify_time_ms INTEGER GENERATED ALWAYS AS
        (CAST(unixepoch(modify_time, 'subsec') * 1000 AS INTEGER)) VIRTUAL,

    -- da-05rkv: what the entry's ops leave contested (JSON, per key each
    -- side's value, device, HLC and op), derived by the sync fold so every
    -- copy holding the same ops holds the same; NULL when nothing is.
    conflicts       TEXT,

    -- `following_state` is only meaningful when `dur` is set. The
    -- parser-level rejection in da-5ns.3 keeps malformed inserts from
    -- ever reaching this layer; the CHECK is a defence-in-depth gate
    -- against accidental writes from future code paths.
    CHECK (following_state IS NULL OR dur IS NOT NULL)
);

-- Unique slug — the human-facing id. Prevents accidental slug collisions on
-- write and makes slug→hid lookups single-hop.
--
-- Partial on `superseded_by IS NULL` (da-uxgv): the belief the table can
-- actually hold is "one LIVE entry per slug", not "one row per slug". Since
-- the sync merge keys on `hid`, two rows with different hids can legitimately
-- want one slug — two devices recording the same field at the same instant,
-- or one corpus imported twice. The loser keeps its own derived slug and steps
-- out of this index instead of having merge state smuggled into the slug.
CREATE UNIQUE INDEX IF NOT EXISTS entries_slug
    ON entries (slug)
    WHERE superseded_by IS NULL;

-- da-ij3x: the lookup index for slug. The partial UNIQUE above can only
-- serve a query that itself says `superseded_by IS NULL`, so every other
-- `slug = ?` — `slug = ? OR hid = ?`, the tombstone probe, the UPDATEs
-- keyed on slug — was a full table SCAN, once per row on the insert path:
-- a 20k-row `db restore` took 105 s. This one covers superseded rows too.
CREATE INDEX IF NOT EXISTS entries_slug_lookup
    ON entries (slug);

-- da-ga8b: partial UNIQUE on (field, occur_time). Slug uniqueness catches
-- the default case where occur_time appears in the slug template, but
-- custom ids (`-K`, `--id`, or `spec.id_template`) can produce distinct
-- slugs that still share the same (field, occur_time) — semantically two
-- state-changes for the same field at the same exact instant, which is
-- undefined. The partial filter excludes tombstones so the soft-delete +
-- re-insert pattern still works.
-- da-uxgv: `superseded_by IS NULL` for the same reason as `entries_slug` —
-- the (field, occur_time) contenders route through supersession too. Custom
-- ids can make two rows collide on one axis and not the other, so a fix that
-- covered only slugs would be rediscovered here.
-- da-2fkr: `state IS NOT NULL`, because the reason above is about STATE. Two
-- rows asserting the state of one field at one instant are undefined; two
-- rows asserting no state at all are two events (two transactions dated to
-- one day) and both belong. A stateless pair that also shares a slug is still
-- refused by `entries_slug` — an id template is the user's declaration that
-- the two differ. The term is the state column, not the verb: a point may
-- carry a state, and then it is the undefined case too.
CREATE UNIQUE INDEX IF NOT EXISTS entries_field_occurtime
    ON entries (field, occur_time)
    WHERE occur_time IS NOT NULL AND deleted_at IS NULL AND superseded_by IS NULL
      AND state IS NOT NULL;

-- Reverse lookup for the conflict marker: "which rows lost to this one?".
-- Partial, so it contains ONLY conflicted rows — empty in steady state, which
-- is what lets the tail ask the question on every render for free.
CREATE INDEX IF NOT EXISTS entries_superseded_by
    ON entries (superseded_by)
    WHERE superseded_by IS NOT NULL;

-- da-05rkv: the entries with conflicts — none in steady state, so listing
-- them is one read of an index that is empty.
CREATE INDEX IF NOT EXISTS entries_conflicts
    ON entries (hid)
    WHERE conflicts IS NOT NULL;

-- da-38obu: the spec rows whose field is a template (`spec:issue__%hid:8%`),
-- which a spec read tries when a field has none of its own: they are few,
-- and the specs of every other field are not read to find them.
CREATE INDEX IF NOT EXISTS entries_spec_templates
    ON entries (slug)
    WHERE field = 'spec' AND slug GLOB '*%*%*';

-- Field-scoped tail queries: most reads are "last N entries for field F" so
-- a composite (field, occur_time DESC) index lets them avoid a sort.
CREATE INDEX IF NOT EXISTS entries_field_occur
    ON entries (field, occur_time DESC);

-- Live-only variant used by the SQL-backed views (R3). The views consistently
-- filter tombstones per field; a partial index lets SQLite skip deleted rows
-- entirely rather than filter them after an index lookup. `occur_time` is
-- nullable (timeless adds, da-lxyw); SQLite indexes the NULL rows too, which
-- is fine — the views read them back as `None`. `slug` is the state
-- order's tie-break (`occur_time DESC, slug DESC`): without it "the newest
-- row before T" sorted the field's whole history to return one (da-asgt4).
CREATE INDEX IF NOT EXISTS entries_live_field_occur
    ON entries (field, occur_time, slug)
    WHERE deleted_at IS NULL;

-- The rows that move a field's state: live, timed, and not an occur dot
-- (`OCCUR_DOT_SQL`, spelled exactly as the readers spell it, or SQLite will
-- not use the index). "The field's newest state row before T" seeks here;
-- on the live index a field of nothing but dots walked its whole history
-- to find there was none (da-asgt4).
CREATE INDEX IF NOT EXISTS entries_field_state
    ON entries (field, occur_time, slug)
    WHERE occur_time IS NOT NULL AND deleted_at IS NULL AND superseded_by IS NULL
      AND NOT (verb = 'point' AND (state IS NULL OR state = 'true'));

-- The blocks among those: the rows a state read looks ahead through for a
-- block still running at its instant (`SPAN_LOOKAHEAD`). On the state index
-- a field with no later block walked every later row to find none, so a
-- past day cost by its distance from now (da-e853a).
CREATE INDEX IF NOT EXISTS entries_field_blocks
    ON entries (field, occur_time, slug)
    WHERE occur_time IS NOT NULL AND deleted_at IS NULL AND superseded_by IS NULL
      AND dur IS NOT NULL
      AND NOT (verb = 'point' AND (state IS NULL OR state = 'true'));

-- The states each field has been logged with (`datum fields`): live,
-- stated, and not a TRACK declaration — `NOT_A_DECLARATION_SQL`, spelled as
-- the reader spells it. The reader skips from one (field, state) to the
-- next, so its cost is the pairs it returns, not the history (da-asgt4).
CREATE INDEX IF NOT EXISTS entries_field_states
    ON entries (field, state)
    WHERE deleted_at IS NULL AND superseded_by IS NULL AND state IS NOT NULL
      AND json_extract(COALESCE(extra_metadata, '{}'), '$.track') IS NULL;

-- Dedup lookup on the hashed content fingerprint.
CREATE INDEX IF NOT EXISTS entries_content_hash
    ON entries (content_hash);

-- Filter by stored verb (e.g. "only switch events in this window").
CREATE INDEX IF NOT EXISTS entries_verb ON entries (verb);

-- da-x3da: sync push filters `WHERE modify_time_ms > <watermark>`. Indexing
-- the virtual generated column materializes the computed epoch-ms so the
-- range filter is a b-tree seek instead of a full-table scan.
CREATE INDEX IF NOT EXISTS entries_modify_time_ms
    ON entries (modify_time_ms);

-- Bare-time tail/head (no `--field` filter) orders by `occur_time`.
-- Without this index the planner falls back to a full SCAN + temp
-- B-tree sort, which dominates at 100k+ entries (see da-7iu). Including
-- `slug` matches the secondary tie-break so the index covers the full
-- ORDER BY. Partial-on-live keeps it skinny.
CREATE INDEX IF NOT EXISTS entries_occur_time
    ON entries (occur_time, slug)
    WHERE deleted_at IS NULL;

-- The listing's order, `-m hybrid`: `occur_time ?? create_time`, so a
-- timeless row sits at its create time (da-2vgj1). Spelled as `time_expr`
-- spells it, or SQLite will not use them: bare tail/head, then a field's
-- (v61).
CREATE INDEX IF NOT EXISTS entries_hybrid
    ON entries (COALESCE(occur_time, create_time), slug)
    WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS entries_live_field_hybrid
    ON entries (field, COALESCE(occur_time, create_time), slug)
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

-- Field specs are not a table: since v38 a field's spec is the entry
-- `spec:<field>`, its data the spec's JSON plus `kit_id` for a kit's spec
-- (da-lc26c; `db/spec.rs`).

-- Views, kit commands and kit index definitions are not tables: since v40
-- each is the entry `view:<name>`, `command:<name>` or
-- `index:<kit>/<name>`, its data the kit's block plus `kit_id` (da-fwwtn;
-- `db/definition.rs`). A kit index's physical SQLite index is this
-- device's, derived from the definitions on open (`Db::derive_kit_indices`).

-- Aliases are not a table: since v39 a user's or kit's alias is the entry
-- `alias:<name>`, its data the alias's JSON plus `kit_id` for a kit's
-- (da-b3752; `db/alias.rs`). Spec-derived aliases live in the `spec`
-- entries and are surfaced lazily via the in-memory SpecAliasIndex.

-- Per-entry modification trail. Append-only. Every mutating operation on the
-- entries table writes a row here in the same transaction. `snapshot` holds
-- the full entry JSON: the newly-created state for version=1 (change_kind
-- 'create'), otherwise the PRE-operation state so a revert can restore it.
CREATE TABLE IF NOT EXISTS entry_history (
    entry_hid         TEXT NOT NULL,
    version           INTEGER NOT NULL,
    snapshot          TEXT NOT NULL,
    modify_time       TEXT NOT NULL,
    raw_input         TEXT,                      -- nullable, mirrors `entries.raw_input` (§5 #11)
    alias_chain       TEXT,
    client_id         TEXT NOT NULL,
    change_kind       TEXT NOT NULL,
    action_id         INTEGER,                   -- the `undo_actions` row that wrote it, if any (v52)
    PRIMARY KEY (entry_hid, version)
);

CREATE INDEX IF NOT EXISTS entry_history_entry
    ON entry_history(entry_hid, version DESC);

CREATE INDEX IF NOT EXISTS entry_history_change_kind
    ON entry_history(change_kind);

CREATE INDEX IF NOT EXISTS entry_history_action
    ON entry_history(action_id) WHERE action_id IS NOT NULL;

-- The undo stack (da-z0zh3; `undo_log.rs`): one row per command that changed
-- entries on THIS copy, its writes the `entry_history` rows naming it, plus
-- `untracked`, a JSON array of hids it wrote with no history (v2import's bulk
-- rows). The cursor is the newest row not `undone`; a new action drops every
-- undone one. `skipped`, a JSON array, is the hids an `undo --skip-changed`
-- left as they are, which its redo leaves too (v60). Local: nothing here syncs.
CREATE TABLE IF NOT EXISTS undo_actions (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    time      TEXT NOT NULL,
    undone    INTEGER NOT NULL DEFAULT 0,
    untracked TEXT,
    skipped   TEXT
);

-- The redo tail, which a new action drops: undone rows alone, so dropping it
-- costs the tail and not every action this copy made (v62, da-qctsr).
CREATE INDEX IF NOT EXISTS undo_actions_undone
    ON undo_actions(id) WHERE undone = 1;

-- Easy Sync (da-k329q). Every change to an entry is a signed op, made here
-- or received from another copy. Append-only: an op is never edited. `body`
-- is the op's canonical JSON, signature included; the other columns are
-- lifted out of it to be queried. `applied` = 0 is an op held until the op
-- before it on its endpoint, and every op it depends on, have arrived.
-- An entry's row is the fold of its applied ops in HLC order (`sync.rs`).
CREATE TABLE IF NOT EXISTS ops (
    endpoint TEXT NOT NULL,
    seq      INTEGER NOT NULL,
    hlc      TEXT NOT NULL,
    hid      TEXT NOT NULL,
    slug     TEXT NOT NULL,
    kind     TEXT NOT NULL,
    applied  INTEGER NOT NULL,
    body     TEXT NOT NULL,
    PRIMARY KEY (endpoint, seq)
);
CREATE INDEX IF NOT EXISTS ops_hid ON ops(hid);
CREATE INDEX IF NOT EXISTS ops_slug ON ops(slug);
CREATE INDEX IF NOT EXISTS ops_hlc ON ops(hlc);
-- The held ops (`applied = 0`), without reading every op's body. Partial,
-- so that no query for applied ops is drawn onto it (da-89ujs).
CREATE INDEX IF NOT EXISTS ops_held ON ops(endpoint, seq) WHERE applied = 0;

-- This copy's endpoint: its Ed25519 key, which signs its ops. LOCAL, never
-- synced, one row. `client_id` and `path` are the client and the file the
-- key was minted on: a file that turns up under another client, or at another
-- path while the original is still there, mints a new key rather than sign
-- as the copy it was copied from (da-p79vq). '' is a key from before v44,
-- whose path is learned on its next use.
-- The secret is stored as it is: the key vouches for this copy's ops on the
-- wire, and anyone who can read the file can sign as it.
-- `paused_at`: `datum sync pause`, this copy's alone; `warned_at`: the last
-- once-a-day warning that its changes have reached no other device (da-u10rl).
CREATE TABLE IF NOT EXISTS sync_local (
    id              INTEGER NOT NULL PRIMARY KEY CHECK (id = 1),
    endpoint_secret TEXT NOT NULL,
    endpoint_id     TEXT NOT NULL,
    client_id       TEXT NOT NULL,
    path            TEXT NOT NULL DEFAULT '',
    paused_at       TEXT,
    warned_at       TEXT
);

-- Where this copy last saw the members of its sync chain (da-rugg8): an
-- address book, granting nothing — only members are synced with (da-6177s).
-- LOCAL, never synced. `addrs` is the iroh address the peer last gave
-- (JSON), to dial it without discovery, or until this copy has seen it
-- (`last_seen`), the one a member passed on (da-fly3f); `announced` is
-- unused since.
-- The last session with it (da-rcb5m, `datum sync status`): the peer's
-- version vector as it ended (JSON), its verify, and the error it failed
-- with (NULL once one succeeds), each with its time, and whether it failed
-- before reaching the peer (`last_error_offline`, da-ueb4p). `addrs` is '' for a
-- peer only `sync record-session` has named (the browser's transport).
-- `redeemed_at`: when this copy redeemed an invite with it and was admitted,
-- where a join cut short resumes (da-38eye).
-- `datum` / `schema`: the build and schema version it last said it runs, in
-- its handshake (da-3zqyv); NULL for a copy that says neither.
CREATE TABLE IF NOT EXISTS sync_peers (
    endpoint_id       TEXT NOT NULL PRIMARY KEY,
    label             TEXT NOT NULL,
    addrs             TEXT NOT NULL,
    added_at          TEXT NOT NULL,
    last_seen         TEXT,
    announced         INTEGER NOT NULL DEFAULT 0,
    last_vector       TEXT,
    last_verify_at    TEXT,
    last_verify_match INTEGER,
    last_error        TEXT,
    last_error_at     TEXT,
    last_error_offline INTEGER NOT NULL DEFAULT 0,
    redeemed_at       TEXT,
    datum             TEXT,
    schema            INTEGER
);

-- A filtered reader's marks (da-u81u1): a member that served it sent every
-- op of `endpoint` up to `seq` that passes `fields` (a sorted JSON array).
-- LOCAL, never synced. Its version vector is, per endpoint, the highest mark
-- whose fields cover its filter now, so a widened filter is sent again.
CREATE TABLE IF NOT EXISTS sync_caught_up (
    endpoint TEXT NOT NULL,
    fields   TEXT NOT NULL,
    seq      INTEGER NOT NULL,
    PRIMARY KEY (endpoint, fields)
);

-- The sync chain's members and admins this copy has heard of (da-odwc7):
-- `announced` once the next command has printed it, and
-- `acked` once `datum sync chain ack` took it off `datum check`. `subject` is an
-- endpoint id, an admin key, or `unlock:<hlc>` (da-aam46). LOCAL, never
-- synced; the chain is in `ops`.
CREATE TABLE IF NOT EXISTS sync_notices (
    subject   TEXT NOT NULL PRIMARY KEY,
    kind      TEXT NOT NULL,
    label     TEXT NOT NULL,
    at        TEXT NOT NULL,
    via       TEXT NOT NULL,
    announced INTEGER NOT NULL DEFAULT 0,
    acked     INTEGER NOT NULL DEFAULT 0
);

-- Data-integrity warnings: overlap / double-open / orphan-end / same-state
-- bookkeeping. Rows are created by the local validator (D5) on insert and
-- by sync-pull reconciliation (C4e); the `detected_by` column distinguishes
-- them. Rows start with `resolution=NULL` and clear naturally when the
-- user fixes the underlying data and detection re-runs against current
-- state; `auto_resolve` remains an internal helper for callers
-- that need to clear obvious cases (tests, future surfaces).
--
-- These are local data-consistency findings — NOT cross-device sync
-- conflicts. They surface patterns like "you logged a stop on a field that
-- was never opened" or "two switches to the same state in a row." A real
-- sync-conflict queue (same entry edited differently on two devices) will
-- be a separate table when sync conflicts arrive.
--
-- `id` is a random 12-char base36 token (full precision); `hid` is the
-- short prefix used at the CLI. `entry_slug` is the slug of the newer
-- triggering entry. `local_entry` holds that triggering entry as JSON;
-- `remote_entry` holds the conflicting counterpart — the older local entry
-- for `local_validator` rows, the incoming remote entry for `sync_pull`
-- rows. May be NULL for orphan_end detection (no counterpart).
CREATE TABLE IF NOT EXISTS integrity_warnings (
    id              TEXT NOT NULL PRIMARY KEY,
    hid             TEXT NOT NULL,
    entry_slug      TEXT NOT NULL,
    local_entry     TEXT NOT NULL,
    remote_entry    TEXT,
    detected_at     TEXT NOT NULL,
    resolution      TEXT,
    resolved_at     TEXT,
    detected_by     TEXT NOT NULL,
    kind            TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS integrity_warnings_hid ON integrity_warnings(hid);
CREATE INDEX IF NOT EXISTS integrity_warnings_slug ON integrity_warnings(entry_slug);
CREATE INDEX IF NOT EXISTS integrity_warnings_pending
    ON integrity_warnings(detected_at) WHERE resolution IS NULL;

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

-- da-4be5x: the state hash a sync's verify sends (`Db::sync_state_hash`),
-- kept until an entry changes, per build and filter (`key`): a page runs
-- its sessions one at a time, and hashing every row cost ~80 s a member at
-- 100k. LOCAL and derived, never synced. Any write to `entries` forgets
-- every hash, whichever path and build writes it; a migration that rebuilds
-- `entries` drops the triggers with it, and runs only under a new build,
-- which reads none of the old build's keys.
CREATE TABLE IF NOT EXISTS sync_state_hashes (
    key     TEXT NOT NULL PRIMARY KEY,
    hash    TEXT NOT NULL,
    per_key TEXT NOT NULL
);
CREATE TRIGGER IF NOT EXISTS sync_state_hashes_insert AFTER INSERT ON entries BEGIN
    DELETE FROM sync_state_hashes;
END;
CREATE TRIGGER IF NOT EXISTS sync_state_hashes_update AFTER UPDATE ON entries BEGIN
    DELETE FROM sync_state_hashes;
END;
CREATE TRIGGER IF NOT EXISTS sync_state_hashes_delete AFTER DELETE ON entries BEGIN
    DELETE FROM sync_state_hashes;
END;
