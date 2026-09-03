# datum-core

Core data model and SQLite persistence for daydumb. This crate is the
authoritative store for entries, field specs, aliases, and CRDT-friendly
metadata; every other crate either depends on it or is explicitly carved
off to remain WASM-clean (see `datum-parse`, `datum-layout`).

## Shape

| Module          | What it owns                                                  |
|-----------------|---------------------------------------------------------------|
| [`db`]          | The [`Db`](db::Db) wrapper around `rusqlite::Connection`.     |
| [`entry`]       | The [`Entry`](entry::Entry) record + verbs (occur/start/stop/switch/note). |
| [`field_spec`]  | [`FieldSpec`](field_spec::FieldSpec): per-field schema, units, defaults. |
| [`slug`]        | Human-readable refs (`field:2024-03-16T09:15Z`), datum-compatible. |
| [`ids`]         | ULID generation and CRDT-safe IDs.                            |
| [`state`]       | Switch-state derivation (current value + transition history). |
| [`views`]       | Read-side projections (now/today/history) consumed by the CLI/web. |
| [`migrations`]  | Embedded SQL migrations, applied at [`Db::open`](db::Db::open). |
| [`history`]     | Audit/history records for entry mutations.                    |
| [`conflicts`]   | CRDT conflict detection + last-writer-wins resolution.        |
| [`last_docs`]   | `_LAST` / `_LAST_WITH_PROTECTION` tokens for quickIds.        |
| [`doc_ref`]     | Stable references between entries (parent/child links).       |
| [`sync_state`]  | Per-DB sync watermarks, cluster id/generation — local `sync_state` table, no network. |
| [`error`]       | The crate-wide [`DatumError`](error::DatumError) enum.        |

## Schema invariants

- Every entry has an immutable ULID (`id`) and a derived human slug.
  Slugs may collide; IDs never do.
- `time_start` and `time_end` are independently optional. All four
  combinations (open, closed, start+dur, end+dur) are valid; duration
  is always derived, never stored.
- `extra_data` is arbitrarily nested JSON. Leaf values may use
  `{"v": x, "u": "unit"}` for unit annotation; bare values are
  unit-less. Both forms round-trip.
- `content_hash` is a SHA-256 over the canonical JSON form and is the
  only safe dedup key — IDs and slugs alone do not catch re-imports.

## Time semantics

`datum-core` re-exports the [`time`] module from `datum-parse` so callers
continue to use `datum_core::time::DatumTime`. The split exists so the
parser and time primitives stay WASM-clean while the SQLite-bound surface
lives here. See [`datum-parse`](../datum-parse/) for the parser side.

## Test helpers

The `test-util` feature exposes shared fixtures (entry/spec/alias
builders, `test_util::fresh_db`, assertion macros) for integration
tests in sibling crates. Production code never sees them.

## Where to look next

- Top-level [README](../../README.md) for the project overview.
- `docs/parity-audit/parsing.md` for how entries are constructed
  from CLI argv.
- `docs/sync-protocol.md` for how entries cross devices via
  [`datum-sync-client`](../datum-sync-client/).
