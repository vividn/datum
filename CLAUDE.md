# daydumb — Agent Context

## What this is

**daydumb** is a personal life-data tracker. The CLI command is `datum`.
Inspired by https://github.com/vividn/datum (a 10-year personal project by the author).

Core purpose: let users manually log data about their own lives and review it as visualized timelines and statistics. The act of logging forces mindfulness. The visualization is "data as art."

## Repo layout

```
crates/
  datum-core/         # Rust library: data model, SQLite, time parsing, field specs
  datum-cli/          # `datum` binary: clap CLI, commands, output formatting
  datum-commands/     # Command-layer crate: typed + string APIs, dispatched by datum-cli
  datum-sync-client/  # Sync client library: wire protocol, crypto, HTTP (WASM-split)
  datum-sync-server/  # Relay server: Rust + axum, E2E-encrypted sync backend
  datum-sync-agent/   # Background sync daemon (planned)
web/            # SvelteKit PWA: static, OPFS + SQLite WASM, offline-capable
```

(Flutter/Android/Wear OS clients are planned but the directories don't
exist yet; legacy datum migration logic lives in
`crates/datum-core/src/migrations.rs` and `commands/db/import.rs`, not
a top-level `migrations/` folder.)

## Core principles (never violate these)

1. **CLI startup must be near-instantaneous** — the original datum (Node.js) took ~1s which is unacceptable. Rust gives us <5ms. Never add synchronous overhead to the hot path (logging an entry).
2. **Local first** — all data stored on device. Network only for sync.
3. **E2E encrypted sync** — the relay server is content-blind. It stores opaque encrypted blobs. Never put decryption logic on the server.
4. **Flexible data model** — both freeform and schema-defined data must work without friction.

## Data model fundamentals

### Entry fields
- `id` (`hid`): internal, prefix-addressable, sortable. **Primary key and the durable identity, local and cross-device** — sync merges key on it and never rewrite it (da-zlug).
- `slug`: `field:2024-03-16T09:15Z` (human reference, datum-compatible). Derived from field + occur_time, so `retime` rewrites it — **mutable, not an identity**.
- `content_hash`: SHA-256 dedup key
- `field`: what is being tracked (e.g. "coffee", "mood", "project", "word")
- `verb`: occur | start | stop | switch | note
- `value`: primary JSON value (state for switch, quantity for occur, etc.)
- `extra_data`: arbitrarily nested JSON; leaf values may use `{"v": x, "u": "unit"}` for unit annotation
- `time_start` / `time_end`: both optional; duration always derived
- `timezone`: IANA name (e.g. "America/Chicago")
- `created_at` / `modified_at` / `device_id`

### Time model
All four combinations are valid:
- `time_start` only — open/ongoing event
- `time_start` + `time_end` — complete block, dur derived
- `time_start` + `--dur` — end derived
- `time_end` + `--dur` — start derived

### Unit annotation in extra_data
Both forms valid:
- `"reps": 10` — bare value
- `"weight": {"v": 75.2, "u": "kg"}` — annotated
CLI parses `weight=75.2kg` → annotated form automatically.

## CLI conventions

### Time flags (datum-compatible)
- `-y` / `-yy` / `-yyy` = 1/2/3 days ago (stackable)
- `-q` / `-qq` / `-qqqq` = 5/10/20 min ago (stackable, 5 min each)
- `-d -1` = yesterday, `-d +1` = tomorrow (day offset)
- `-t -8` = 8 min ago, `-t +5` = 5 min ahead (minute offset)
- `-t 1513` = 15:13 today, `-t 9:15am` = natural time
- `--dur 30m` = duration (30 min, 1:30, 2h, ISO PT30M all work)

### Syntax variants (both must work)
- `datum switch mood happy`  ← verb-first
- `datum mood switch happy`  ← field-first (swap in pre-pass before clap)

### Alias system (built-in, not shell aliases)
Stored in the DB. Resolved in the CLI pre-pass before clap sees argv.
- `datum alias set vbp "note word srcLang=pt pos=verb"`
- Then `datum vbp correr "to run"` expands to `datum note word srcLang=pt pos=verb correr "to run"`

## Sync architecture

- Clients encrypt data with AES-256-GCM before upload
- Relay server stores opaque base64 blobs + metadata (device_id, seq)
- Sync = push new changesets, pull changesets from other devices since last seq
- Auth: Passkeys (WebAuthn) — no passwords
- Key management: encryption key stored locally + wrapped copy on server (encrypted with recovery passphrase)

## Web app (PWA)

- SvelteKit static adapter — no server-side rendering
- SQLite WASM + OPFS for local storage (same schema as CLI)
- Service Worker caches app shell for offline use
- Background Sync API queues sync when offline

## Migration from original datum

The original datum uses CouchDB JSON documents. Backup format:
```json
{ "backupTime": "...", "docs": [...CouchDB docs...] }
```
Each doc has `data.field`, `data.occurTime`, `data.state`, `data.dur`, `meta.createTime`.
Migration: `datum db import --from backup.json`

## Running tests

```bash
cargo test              # all crates
cargo test -p datum-core
cargo test -p datum-cli
```

All 22 Phase 1 tests must pass. Add tests for new behaviors.

## Pre-submit

Before opening a PR, run the local mirror of the CI battery:

```bash
bash scripts/check.sh
```

It runs `cargo fmt --check`, `cargo clippy -D warnings`, the workspace build,
the test suite (nextest when installed, else `cargo test`), and the doc tests —
the same set CI gates on. For fast iteration on a small change,
`scripts/check-changed.sh` runs the same battery scoped to the crates the diff
touches (falling back to the full battery on workspace-root config changes or
when the dependent closure is large).

## Building

```bash
cargo build --release -p datum-cli   # produces target/release/datum
```

## Web app dev

```bash
cd web && npm install && npm run dev   # localhost:5173
```
