# daydumb — Agent Context

## What this is

**daydumb** is a personal life-data tracker. The CLI command is `datum`.
Inspired by https://github.com/vividn/datum (a 10-year personal project by the author).

Core purpose: let users manually log data about their own lives and review it as visualized timelines and statistics. The act of logging forces mindfulness. The visualization is "data as art."

## Repo layout

```
crates/
  datum-core/         # Rust library: data model, SQLite, field specs
  datum-parse/        # Pure: argv + time parsing, verb truth table, entry identity
  datum-parse-wasm/   # datum-parse for the browser (argv -> the row to insert)
  datum-cli/          # `datum` binary: clap CLI, commands, output formatting
  datum-commands/     # Command-layer crate: typed + string APIs, dispatched by datum-cli
  datum-layout/       # Pure: dayview/nowview geometry + SVG render
  datum-layout-wasm/  # datum-layout for the browser
  datum-sync-client/  # Sync client library: wire protocol, crypto, HTTP (WASM-split)
  datum-sync-server/  # Relay server: Rust + axum, E2E-encrypted sync backend
  datum-sync-agent/   # Background sync daemon (planned)
web/            # SvelteKit PWA: static, OPFS + SQLite WASM, offline-capable
```

(Flutter/Android/Wear OS clients are planned but the directories don't
exist yet; legacy datum migration logic lives in
`crates/datum-core/src/migrations/` and `commands/db/import.rs`, not
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
- `time_start` + a duration — end derived
- `time_end` + a duration — start derived

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
- `-d yesterday` / `today` / `tomorrow`; `-d sunday` = the most recent
  Sunday, `-d sunday+` = the coming one (trailing `+`/`-` forces the
  direction, and applies to `apr29`-style dates too)
- `-t` accepts anything `-d` does — day words, weekdays, `2024-03-16` —
  so a caller holding "either a time or a date" needs only one flag
- a duration is a trailing positional (`datum end work 30m`) or `dur=30m`
  (`1:30`, `2h`, ISO `PT30M` all work). It anchors the block AT the entry's
  time and runs forward. There is no `--dur` flag — da-azqq removed it
  because it ran the block backward from `-t` where the other two spellings
  ran forward; `crates/datum-cli/tests/dur_flag_removed.rs` keeps it gone

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
- One connection per origin, not per tab (da-eovo). The OPFS SAH pool is
  single-writer, so exactly one `db.worker.ts` opens the database — the one
  that wins `navigator.locks.request('datum-db-owner', …)`, a lock its
  callback never releases, so it is held for that tab's life. Every other
  tab's worker opens nothing and relays its `DbRequest`s to the owner over a
  `BroadcastChannel`, so from `db.ts` upwards nothing changed and two tabs
  are both durable. Unanswered requests sit in the worker's `inflight` map
  and are sent again when a new owner announces itself — that queue is what
  makes "nothing typed is ever lost" true across an owner tab closing. A
  browser with no `BroadcastChannel` or no Web Locks degrades to the old
  behaviour (second tab in memory, and it says so), never to a lost row.
  `web/src/lib/changes.ts` carries the separate "rows changed" signal that
  makes each tab re-run its panes.
- Service Worker caches app shell for offline use — the shell, everything
  `$service-worker` lists, and Vite's separate worker bundle (the SQLite
  worker and sqlite3.wasm), which that list does not know about and which
  the `datum-worker-precache` plugin in `web/vite.config.ts` enumerates
  instead. `web/tests/offline.test.mjs` reloads the app with the network
  down and logs an entry, so the precache cannot silently lose a file
  again (da-d1l9).
- Background Sync API queues sync when offline
- The dayview strip is a stack of days, not one band (da-cgyh): today is
  pinned at the bottom and a pull brings the days before it down above,
  one `dayview` render per band through the seam (`bandArgv` in
  `web/src/lib/panes.ts`). Every band asks for the same `--width` and
  `--day-height`, which is what pins `label_width` and so makes the hour
  gridlines of one band land on those of the next; all but today's
  suppress the axis strip with `--time-axis-height 0`. When the band
  above today is not the day before it, a jagged `<button>` between them
  scrolls yesterday back down.

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
cd web && pnpm install && pnpm run dev   # localhost:5173
pnpm test                              # the seam suite (builds first)
pnpm run test:fixtures                 # recapture the CLI-parity fixtures
```

Two regions of the app are datum's own Rust compiled to
`wasm32-unknown-unknown`, not a second implementation of it:

- **the parser** — `crates/datum-parse` via `crates/datum-parse-wasm`
  (da-7scm). argv in, the row to insert out: time flags, `KEY=VALUE`
  grammar, duration spellings, the verb truth table, the slug and the
  content hash all come from the CLI's own code. `web/src/lib/parser.ts`
  is the browser half; there is no TypeScript parser any more.
- **the dayview / nowview panes** — `crates/datum-layout` via
  `crates/datum-layout-wasm` (da-6h6p), reached from
  `web/src/lib/layout.ts`. Rows in, SVG out: which mark each entry makes
  is the wasm crate's `marks.rs` dispatching on the CLI's own verb→shape
  table (`datum_parse::shape`, moved out of `datum-core` by da-t7ud so
  wasm can reach it), and the colours and geometry were already Rust's.
  `crates/datum-commands/src/render/wasm_parity.rs` holds that walk — and
  the colour helpers — against `field_visuals::render_field_visuals`.

Both use the same five-function C ABI, loaded by `web/src/lib/wasm.ts`.
Neither needs wasm-bindgen, and neither may acquire an import: a module
with imports will not load from the bare `WebAssembly.instantiate` the
browser side uses (this is why the workspace turns off chrono's
`wasmbind`). Both artifacts are committed under `web/static/` the way the
fixtures are, so the web CI job needs no Rust toolchain — rebuild and
commit them with `pnpm run build:wasm` whenever the crates behind them
change.

Forgetting that is not a silent failure any more (da-nig7). CI's
`wasm-check` job re-runs `build:wasm` and fails on any byte of difference,
and `build-and-test` does the same for the parity fixtures with
`pnpm run test:fixtures`. Both comparisons are byte-for-byte, which the
wasm build earns by remapping the absolute paths rustc bakes into panic
payloads — so the artifact does not depend on where the repo or the cargo
registry lives, and a runner reproduces what your machine produced.

What the browser still assembles itself is the small DB-free part of
`log_core` (`crates/datum-parse-wasm/src/log.rs`), and
`crates/datum-commands/tests/wasm_parse_parity.rs` drives the real
`log_entry` and that module over the same argv matrix so the two cannot
drift. Log shapes it does not assemble — `add`/`note`, `-K`, `-R`, `,`
chains — report themselves rather than storing something else.

`web/tests` drives the built app in a real browser and asserts through
`run(argv)` — the one seam every region of the app goes through — rather
than through the local runner behind it, which da-1vat deletes.
`tests/parity.test.mjs` compares what a given argv stores in the browser
against what the real `datum` binary stored for the same argv, and every
case is a hard assertion: since the parser is shared there is nothing for
a per-case exemption to excuse. Needs a headless browser once: `npx
playwright install --only-shell chromium`.
