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
web/            # SvelteKit PWA: static, the real CLI as WASI on OPFS, offline-capable
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

What the row actually stores is `occur_time` + `dur`, and `occur_time` is
the block's END: the duration is the span that PRECEDES it (da-zwct,
matching the original datum's `durationBlocks` view). So a `(start, end)`
pair lands as `occur_time = end`, and a bare `start` with no end — an
open block — is the one shape whose `occur_time` is its leading edge,
because it has no other instant to store.

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
  (`1:30`, `2h`, ISO `PT30M` all work). It says how long the thing LASTED,
  so the block ENDS at the entry's time and the span precedes it —
  `datum start posture 4` means posture was happening for the past four
  minutes, and `-t 1400` with `30m` is 13:30–14:00 (da-zwct; the original
  datum's `durationBlocks` view). There is no `--dur` flag — da-azqq
  removed it because the three spellings disagreed about which side of
  `-t` the block fell on; `crates/datum-cli/tests/dur_flag_removed.rs`
  keeps it gone, and the two survivors now agree with the reference

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
- The real `datum` CLI is the only runner (da-g8o8): `datum-cli` built for
  wasm32-wasip1, one WASI process per command in `web/src/lib/db.worker.ts`
  (`cli.ts`, `wasiFs.ts`), its SQLite file in OPFS. There is no SQL, parser
  or renderer in TypeScript — every pane, the widget bar (`fields --json`),
  the tail table (`tail --json`'s `columns`), /now (`now --bare`) and the
  export button (`db backup -`) are CLI commands through `run(argv)`.
- Every process takes the database files for itself (sync access handles
  opened at start, closed at exit) under the `datum-cli-files` Web Lock, so
  tabs take turns fairly and none is the owner; the SQLite header's change
  counter (offset 24) decides whether a process wrote, which is what
  `web/src/lib/changes.ts` tells the other tabs. A browser that refuses the
  handles gets a database in memory for the tab, which keeps going back for
  the file and moves its rows across with `db backup -` / `db restore -`.
  SQLite runs in rollback-journal mode under the shim (WASI has no WAL); a
  journal left by a killed process is rolled back by the next one.
- Service Worker caches app shell for offline use — the shell, everything
  `$service-worker` lists, and Vite's separate worker bundle (the database
  worker and datum-cli.wasm), which that list does not know about and which
  the `datum-worker-precache` plugin in `web/vite.config.ts` enumerates
  instead. `web/tests/offline.test.mjs` reloads the app with the network
  down and logs an entry, so the precache cannot silently lose a file
  again (da-d1l9).
- Background Sync API queues sync when offline
- The dayview strip is a stack of days, not one band (da-cgyh): today is
  pinned at the bottom and the days before it are stacked above, as many
  as the strip's box holds (da-zb0d: the height comes from the container,
  the day height is a `dayHeight` prop with a `change` event, and the
  strip keeps nothing in `localStorage`), one `dayview` render per band
  through the seam (`bandArgv` in `web/src/lib/panes.ts`). Every band
  asks for the same `--width` and `--day-height`, which is what pins
  `label_width` and so makes the hour gridlines of one band land on those
  of the next; all but today's suppress the axis strip with
  `--time-axis-height 0`. When the band above today is not the day before
  it, a jagged `<button>` between them scrolls yesterday back down.

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

The browser runs `datum-cli` itself, compiled to wasm32-wasip1 by
`pnpm run build:cli-wasm` (pinned wasi-sdk and binaryen, fetched into
`target/toolchains/`) and loaded through `@bjorn3/browser_wasi_shim`. What
the page supplies is what a shell would: argv, stdin, `TZ`, a device id,
`DATUM_COLOR=1` (there is no tty), and `HOME`/`DATUM_DB` inside the
preopened `/data`. The artifact is committed under `web/src/lib/wasm/` the
way the fixtures are, so the web CI job needs no Rust toolchain — rebuild
and commit it whenever the CLI's crates change. It lives under `src/`
rather than `static/` so the app reaches it as a `?url` import and Vite
emits it content-hashed under `/_app/immutable/`: a runtime artifact must
not have a stable name, because the service worker serves precached names
cache-first and `deploy.sh` marks hashed assets `immutable` for a year, and
together those handed a returning browser the previous deploy's module to
run the current deploy's JavaScript against (da-tvg0). `deploy.sh
--check-dist <dir>` is that rule on its own.

Forgetting to rebuild is not a silent failure (da-nig7). CI's `wasm-check`
job re-runs `build:cli-wasm` and fails on any byte of difference, and
`build-and-test` does the same for the fixtures with `pnpm run
test:fixtures`. Both comparisons are byte-for-byte, which the build earns by
remapping the absolute paths rustc and clang bake into the bytes.

(The `datum-parse-wasm`, `datum-layout-wasm` and `datum-render-wasm` crates
built the three modules the browser ran before the swap; nothing in `web/`
loads them any more.)

`web/tests` drives the built app in a real browser and asserts through
`run(argv)` — the one seam every region of the app goes through.
`tests/parity.test.mjs` compares what a given argv stores in the browser
against what the native `datum` binary stored for the same argv, and
`tests/stdout.test.mjs` what a script of commands PRINTS, byte for byte
after masking hids and write times. Both are hard assertions: the browser
runs the CLI, so there is nothing for an exemption to excuse. Needs a headless browser once: `npx
playwright install --only-shell chromium`.
