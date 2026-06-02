# Views — Parity Audit + Refactor Plan (daydumb vs datum)

> **⚠️ Pre-pivot reference (2026-05-01).** This audit was written against
> the v=19 schema (`start_time` + `end_time` + `value`). The **da-5ns**
> time-shape pivot replaced that with `occur_time` + `state` + `dur` +
> `following_state` and a stored five-verb classifier. Column names and
> SQL fragments below refer to the pre-pivot shape; the *behaviour* still
> applies, but the SQL must be re-projected onto the v=20 columns when
> referencing this doc. A v3-aligned rewrite is filed as follow-up
> (out-of-scope for sub-bead 8).

This document audits datum's CouchDB views against daydumb's SQL-backed
replacements (post-R3, `da-t6t.3`). For each view ported into daydumb we
record the original map/reduce, the new SQL form, and any divergences;
unported datum views are listed with a disposition.

Reference points:
* daydumb views: `crates/datum-core/src/views/`
* daydumb schema: `crates/datum-core/src/schema.sql`
* daydumb migrations: `crates/datum-core/src/migrations.rs`
* datum source: `/tmp/datum-survey/src/views/datumViews/`

The pre-R3 daydumb views hand-rolled CouchDB's map/reduce mental model on
top of `db.all_entries()`: each view materialised a `Vec<ViewRow>` by
iterating every live entry (and every tombstone) and then let a generic
framework filter/sort/group/reduce the results. R3 replaces that with
per-view SQL queries scoped to the live rows of interest; the public
`Registry::query(name, db, params)` surface is unchanged.

---

## Ported views (datum → daydumb)

### `datum_timing`

* **datum source**: `src/views/datumViews/timingView.ts`
* **daydumb impl**: `crates/datum-core/src/views/timing.rs`
* **daydumb trait surface**: `impl View for TimingView` with a single
  `query(db, params)` method.

**datum emit shape:**

* map key: `[TimeMetric, field | null, isoUTC]` where `TimeMetric` is one
  of `hybrid | occur | create | modify`. Each entry emits up to 8 map rows
  (two per metric × two field-presence values).
* map value: `[localDate, epochMs | null]`
* reduce: `_count`

**daydumb emit shape (simplified from datum):**

* key: `[field, localDate]` — one key per entry, `time_start` only.
* value: `{ start: iso_utc, end: iso_utc | null, duration_ms: int | null }`
* id: the source entry's slug.
* reduce: `_count`.

**Why simplified:** daydumb canonicalises occur/hybrid onto `time_start`
and carries the block's end+duration in the value instead of splitting
across metrics. The per-metric fan-out (hybrid/occur/create/modify) is
deferred until a caller demonstrates need — nothing in daydumb's callers
(`datum map`, `datum reduce`, `datum dayview`) uses non-occur metrics.

**SQL form:**

```sql
SELECT slug, field, start_time, start_time_offset, end_time
FROM entries
WHERE deleted_at IS NULL
  AND start_time IS NOT NULL
  [AND field = ?]             -- pushed down when key/startkey/endkey
                              -- narrow to a single field
```

Local-date and duration_ms are computed in Rust from the returned
columns. Group/reduce (`_count`) and key-range filtering run in Rust over
the SQL result set, since the cardinality is bounded by live timestamped
entries for a single field.

**Required index:** `entries_field_start (field, start_time DESC)` — pre-existing. Partial `entries_live_field_start` (added in migration v5→v6) narrows to live+timed rows.

**Edge cases / divergences from datum:**
* datum's `subtractSecondsFromTime` ISO-duration parsing doesn't apply; daydumb stores `time_end` explicitly.
* Tombstones and entries without `time_start` are excluded at the SQL level, not in Rust.
* Emit count is 1 per live timed entry (datum can emit up to 8).

**Tests:** `timing::tests::timing_view_emits_entries_with_durations`, `timing_view_reduce_counts_entries`, `timing_view_pushes_field_filter_to_sql`, `timing_view_respects_limit`.

---

### `state_change`

* **datum source**: `src/views/datumViews/stateChangeView.ts`
* **daydumb impl**: `crates/datum-core/src/views/state_change.rs`

**datum emit shape:**

* map key: `[field, isoUTC]`
* map value: `[lastState, activeState]`
* map reads explicit `data.lastState` / `data.dur` / `data.state` fields per doc and handles ISO-8601 duration math to derive block-begin events.
* reduce: `_count`

**daydumb emit shape (same as pre-R3):**

* key: `[field, rfc3339_utc_time]`
* value: `{ from: DatumState, to: DatumState }`
* id: source entry's slug.
* reduce: `_count`.

**SQL form:**

```sql
SELECT slug, field, verb, value, start_time, end_time, create_time
FROM entries
WHERE deleted_at IS NULL
  AND verb IN ('switch','start','stop','occur','block')
  AND (start_time IS NOT NULL OR end_time IS NOT NULL)
  [AND field = ?]
ORDER BY field ASC, COALESCE(start_time, end_time) ASC, create_time ASC
```

The per-field rolling prior-state (the LAG-style fold) stays in Rust:
state normalisation via `crate::state::normalize_state` is non-trivial
(arrays, `{id: ...}` envelopes, boolean coercion) and best expressed
idiomatically over the ordered SQL result. Each entry can fan out to
either 1 event (`start`/`switch` with no `end_time`, `stop`, `occur`) or
2 events (paired start+end) — still bounded linearly by live rows.

**Required index:** `entries_field_start`. The new partial
`entries_live_field_start` skips tombstones entirely for this scan.

**Edge cases / divergences:**
* datum rules around `data.lastState` (explicitly supplied by caller) have
  no daydumb analogue — we derive `from` from the rolling prior via the
  verb's intrinsic semantics.
* datum emits "hole" blocks when `data.dur` is negative (corrective rows
  that flip state back via negative durations). daydumb doesn't synthesise
  those; R3 keeps the pre-R3 behaviour.
* `add` entries never surface (map/reduce framework vs SQL both skip them).

**Tests:** `state_change::tests::state_change_view_emits_transitions`, `state_change_view_skips_tombstoned_and_noteonly`, `state_change_pushes_field_filter`.

---

### `duration_sum`

* **datum source**: `src/views/datumViews/durationSumView.ts`
* **daydumb impl**: `crates/datum-core/src/views/duration_sum.rs`

**datum emit shape:**

* map key: `[field, DatumState, isoUTC]` (state is part of the key)
* map value: seconds (signed; negative rows cancel positive rows in `_sum`)
* reduce: `_sum`

**daydumb emit shape:**

* key: `[field, localDate]`
* value: `int` (milliseconds) — derived from `end_time - start_time`.
* id: source entry's slug.
* reduce: `_sum`.

**Why different:** daydumb doesn't track state in the duration key. The
canonical `time_start` + `time_end` pair is authoritative, so we sum the
wall-clock duration directly. datum's `[field, state, time]` decomposition
was an encoding hack for CouchDB's GROUP BY (state needed to be part of
the key to split totals by state); daydumb's callers don't need state-split
totals, and if they eventually do, it's a follow-up view.

**SQL form:**

```sql
SELECT slug, field, start_time, start_time_offset, end_time
FROM entries
WHERE deleted_at IS NULL
  AND start_time IS NOT NULL
  AND end_time IS NOT NULL
  [AND field = ?]
```

Duration_ms computed via `(end_time - start_time).num_milliseconds()`;
`_sum` reduce with grouping (`group=true`, `group_level=1`, or ungrouped)
handled in Rust. For the typical "total duration for field X over day Y"
case the candidate set is small; SQL aggregation via `SUM` and `GROUP BY`
would be a straightforward follow-up if profiling shows need.

**Required index:** `entries_field_start`; `entries_live_field_start` (new) is strictly better here.

**Edge cases / divergences:**
* Negative-duration "hole" rows are not emitted (pre-R3 parity).
* Entries without both start and end are silently skipped (datum does the
  same, falling through the `!dur` branch).

**Tests:** `duration_sum::tests::duration_sum_view_reduces_by_field_and_day`, `duration_sum_pushes_field_filter`.

---

### `datum_sub_human_id`

* **datum source**: `src/views/datumViews/subHumanIdView.ts`
* **daydumb impl**: `crates/datum-core/src/views/sub_human_id.rs`

**datum emit shape:**

* map key: `hid.slice(0, i)` for i = hid.length..1 — every non-empty prefix of the full `meta.humanId` string.
* map value: `null`
* reduce: `_count`

**daydumb emit shape:**

* key: prefix string (length 1..=`SHORT_HID_LEN`=5) of the entry's
  `short_uid()`
* value: the full `short_uid()` string (for downstream display).
* id: source entry's slug.
* reduce: `_count`.

**Why bounded at 5 chars:** daydumb's `hid` is a 12-char base36 random
and `short_uid` = first 5 chars is what the CLI prints. Ambiguity beyond
5 chars is vanishingly rare, and any full-hid lookup goes through
`Db::resolve_prefix` (which queries the PK directly) rather than through
this view.

**SQL form:**

```sql
SELECT slug, substr(hid, 1, 5) AS short_hid
FROM entries
WHERE deleted_at IS NULL
  [AND substr(hid, 1, 5) LIKE ? || '%' ESCAPE '\']
  -- pushed down when --key (or shared --startkey/--endkey prefix) narrows
```

Prefix fan-out (length 1..=5 per entry) happens in Rust after SQL filters
down to only entries whose short-hid already starts with the caller's
prefix. For the shortest-unique-prefix workflow this makes each lookup
O(matching entries) instead of O(all entries × 5).

**Required index:** primary key on `hid` (pre-existing). SQLite uses the B-tree to seek `substr(hid, 1, 5) LIKE <prefix>%` via the implicit `hid LIKE <prefix>%` relationship. No new index needed.

**Edge cases / divergences:**
* Tombstones are excluded at the SQL level.
* Values are the short_uid string (useful downstream); datum emits `null`.

**Tests:** `sub_human_id::tests::sub_human_id_view_returns_shortest_unique_prefix`.

---

## Views in datum not in daydumb

The following datum views exist in `/tmp/datum-survey/src/views/datumViews/`
but have no Rust counterpart. Disposition below.

| datum view (TS)            | Purpose                                            | Disposition |
|----------------------------|----------------------------------------------------|-------------|
| `humanId.ts` (`datum_human_id`) | One row per doc keyed on `meta.humanId`.     | **Drop**: replaced by `entries.slug` unique index + `Db::get_entry`. |
| `structure.ts` (`structures`)   | Per-doc `[idStructure, fieldStructure]`.     | **Drop (for now)**: template metadata lives in `entries.id_structure/field_structure`; callers can SELECT. |
| `overlappingBlocks.ts`     | `+1/-1` at block boundaries for overlap detection. | **Defer**: daydumb overlap detection is in `sync_conflicts` (`detected_by='local_validator'`). File a follow-up if block-style detection is ever needed. |
| `durationBlocks.ts`        | Per-block `(field, blockBegin, blockEnd)`.         | **Defer**: duplicative with `duration_sum`; port only on demand. |
| `datumV1.ts`               | Legacy DatumV1 migration helper.                    | **Drop**: daydumb migrates via `datum db import --from backup.json` (TS-free). |
| `idToHumanView.ts`         | Map `docId → humanId`.                              | **Drop**: daydumb slug *is* the human id. |
| `dataStructuresView.ts`    | JSON-shape survey of `doc.data` fields.             | **Defer**: no current caller; useful only for schema discovery. |
| `idStructures.ts`          | Group by `meta.idStructure` template.               | **Defer**: spec templates tracked in `field_specs` table. |
| `typeStructure.ts`         | Group by inferred value types.                      | **Defer**: not used by any shipped CLI surface. |
| `fieldView.ts` (`datum_fields`) | Distinct field names + counts.                  | **Drop**: replaced by `Db::distinct_fields` (direct SQL `SELECT DISTINCT field`). |
| `pointDataView.ts`         | Per-point-type data slice.                          | **Defer**: not used by shipped callers. |

User reviews this list; **defer** = don't port unless a concrete caller asks; **drop** = the datum view is structurally redundant with daydumb's schema or already covered by a direct DB method.

---

## Performance notes

All four ported views previously scanned every row returned by
`Db::all_entries()` (full `SELECT *` + deserialisation into `Entry` +
in-Rust tombstone filter). Post-R3:

* **Scan cost**: SQL fetches just the columns needed per view (5–7 each
  vs ~25 on a full `Entry`). Tombstones and non-matching entries are
  excluded at the SQL level.
* **Field pushdown**: when a caller supplies `--key` (or matching
  `--startkey`/`--endkey`) whose first element is a field name, that
  filter reaches SQL and uses `entries_field_start` (or
  `entries_live_field_start`). Pre-R3 this was a full-table scan.
* **Partial index (new)**: `entries_live_field_start (field, start_time)
  WHERE deleted_at IS NULL AND start_time IS NOT NULL` added in migration
  v5→v6. Covers three of the four views whose common shape is
  `WHERE live AND start_time IS NOT NULL [AND field = ?]`.
* **Reduce/group**: still Rust-side for all views. Cardinality after SQL
  filtering is low enough that SQL GROUP BY was judged not worth the
  per-view complexity. Promotable to SQL aggregation if profiling shows
  it matters.

No micro-benchmarks were run as part of R3; the refactor is
structurally-directed (stop materialising every live entry × every view).

---

## Ideas / cleanup

* **SQL `CREATE VIEW` candidates**: none of the four currently implemented
  views are good `CREATE VIEW` candidates — each embeds format decisions
  (local_date string, `{start,end,dur_ms}` JSON, prefix fan-out, rolling
  state fold) that belong in Rust, not SQL. A future `entries_live` view
  abstracting the `WHERE deleted_at IS NULL` guard would simplify every
  view's SQL.
* **Parameterised-SQL-vs-GROUP-BY**: if reduce becomes a hot path, moving
  the `_count`/`_sum` reductions into SQL (`COUNT(*)` / `SUM(...)` +
  `GROUP BY`) is straightforward per view. The group_level expression
  would need a per-view SQL snippet (e.g. `GROUP BY field` for level 1,
  `GROUP BY field, local_date` for level 2/full).
* **Droppable**: the `group_key`/`filter_by_key`/`reduce_*`
  helpers in `views/mod.rs` are kept because each view calls them; if
  reduce moves into SQL the helpers shrink to `filter_by_key` only.

---

## Summary: acceptance checklist for R3

- [x] Every pre-R3 view has a SQL-backed implementation.
- [x] Map-reduce scaffolding removed: `View::map` / `View::reduce` /
      `supports_reduce` are gone; trait exposes a single `query()` method.
- [x] Partial `entries_live_field_start` index added via migration v5→v6.
- [x] `docs/parity-audit/views.md` exists with per-view SQL + divergences +
      disposition for unported datum views.
- [x] `cargo test --workspace` passes (see R3 commit).
- [x] `Registry::query(name, db, params)` public API unchanged —
      `datum map` / `datum reduce` / `datum dayview` call sites untouched.

---

*Last touched against `main` @ `4db4903` on 2026-06-02 (vremya/parity-audit-footer). Drift is expected between touches; if a claim here disagrees with code, the code wins. To refresh: walk the doc against current `main`, update the items, bump this footer's sha + date in the same commit.*
