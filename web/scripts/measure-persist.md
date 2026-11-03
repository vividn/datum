# Where a datum.day command's time goes

Research for da-zv6hn. vividn, 2026-11-03: «every separate widget has to reopen a wasm connection all
the time right? Are they doing that in parallel? can we just have a persistent wasm connection that
everyone shares?» And: «Keeping the web snappy and performant is a high priority because it's one of
the main pillars of local-first software.»

**Short answer.** Yes, each command is a process of its own. They run one after another, never in
parallel. About 10 ms of each process is set-up that a long-lived instance would skip. For a desktop tap
and its two sync sessions (29 processes) at 290k entries, one persistent instance cut the CLI time from
**567 ms to 199 ms**. That is the upper bound, measured with a loop that has no request delivery yet.
Parallel reads are possible only beside other reads, never beside a write. The persistent instance gets
most of their gain at much less risk.

## How it was measured

- The fixture is `measure-log.mjs`'s 290,000 entries, rebuilt at staging 9911be2 (da-yo698's head):
  copy B, a member of A's chain, 1.29 GB.
- The tap is `measure-log.mjs --layout desktop --presses 4 --json`. Its fourth tap is a `walk` button
  with panes for the day strip, tail, fields, nowview and stats. Each process was recorded at the
  worker's door, argv, env and stdin whole (the one change to `measure-log.mjs`). The tap ran 29
  processes: the write, the bar's three reads, six pane refreshes, the leader's `sync pipe vector`, and
  the 19 processes of the two sessions (ours, then the peer's push back).
- `measure-persist.mjs` replays those 29 processes in a blank, cross-origin-isolated page through the
  app's own `db.worker`. There is no layout, no panes and no leader, so each process is alone in the
  queue. Medians are of 7 runs, and the OPFS copy is seeded afresh before each part.
- `measure-persist.patch` instruments the CLI. It is applied for the measurement and not committed to the
  crates:
  - `DATUM_PHASES=1` prints marks at each `Db::open` and its `migrate` steps, after the parse, after the
    dispatch and at exit, with the wasm linear memory size.
  - `DATUM_LINES=1` is the prototype: one `Db` opened once. Each stdin line is one command
    (tab-separated argv, leading `=KEY=VAL` words its env), parsed and run as a REPL line is
    (`parse_invocation_with_db`, `run_on`), with its own marks.
- The box had 24 cores and a load of 8–21 during the runs. The page was Chromium 153 headless. The phone
  layout was not run: its commands are a subset of these.

## 1. Where a process's time goes

Mean over the 29 processes, alone, in the browser at 290k:

| phase | ms | what it is |
|---|---|---|
| acquire | 4.1 | the Web Lock and four OPFS handles, paid once per burst (da-dvzx). Here every process paid it, since each ran alone |
| instantiate | 1.4 | a new `WebAssembly.Instance` of the compiled module |
| first `Db::open` | 3.3 | `args::parse_invocation` opens the database for the alias lookup and drops it |
| second `Db::open` | 2.7 | `dispatch_single` opens it again |
| … of which, both opens | 2.8 / 1.75 / 0.5 / 0.4 / 0.5 | connection and PRAGMAs / the five DDL batches of `migrate` / version check / kit indices / refold, create-time and symbol passes |
| parse | 0.2 | |
| dispatch | 1.5–47 | the command itself |
| exit + flush | 0.5 (writes 4) | handles flushed; a write's OPFS flush is ~3.5 |
| the worker's own | 1.0 | messages, change counters, `keep` |

So **each process carries ~10 ms of fixed cost**, 14 when it acquires, around a dispatch that is often
smaller. For the session's commands the dispatch is 1.8–4 ms (`vector`, `known`, `handshake`, `ops
--since`) and the process is 13–17 ms. Over the tap:

| | processes | CLI time, alone (excluding acquire) | of which dispatch | fixed |
|---|---|---|---|---|
| the tap: write, bar reads, panes, leader's vector | 10 | 329 ms | 214 | 115 |
| its two sync sessions | 19 | 238 ms | 62 | 176 |

Per command (alone wall, including acquire; dispatch):

| command | wall | dispatch | as a line (2.) |
|---|---|---|---|
| `end walk` (the write) | 53.3 | 33.8 | 20.6 |
| `fields --json` | 23.3 | 8.1 | 5.1 |
| `now --json` | 30.6 | 10.9 | 8.9 |
| `conflicts --json` | 13.9 | 0.2 | 0.2 |
| `dayview` (the strip's band) | 48.3 | 31.5 | 20.2 |
| `tail --json -n 116` | 28.5 | 15.6 | 12.4 |
| `stats --month --show all` | 43.3 | 26.7 | 18.3 |
| `dayview --data-layer … --pictures 0` (today) | 49.6 | 37.8 | 32.1 |
| `dayview --data-layer … --digests 0` (yesterday) | 68.6 | 47.3 | 37.9 |
| `sync pipe vector` | 13–16 | 1.5–2.1 | 0.9–1.2 |
| `sync pipe verify` | 25–34 | 9–13 | 6–9 |

The raw numbers are in the run's `--json`, with each command's worst run and peak memory beside its
median.

## 2. A persistent instance

**What it saves.** The 29 processes as lines of one process: **567 → 199 ms** (the tap 329 → 157, the
sessions 238 → 42). Two things make up the difference:

- Every fixed cost above but one open is gone: one open per instance (3.7 ms), not two per command.
- The dispatch itself is 20–50% faster on a warm SQLite cache and warm statements (`dayview` 31.5 →
  20.2, `now` 10.9 → 8.9).

This is an upper bound. The loop gets every line on stdin at once, while a real instance gets each
request by message. Delivery should cost about one `postMessage` (~1 ms, the worker's own above); that
was not measured.

As arithmetic, not a measurement: the desktop tap's queue at these per-line costs plus 1.5 ms a request
has the tail answered at ~55 ms, the nowview's layer at ~130, yesterday's digest at ~170, and the
session's first `ops --since` at ~180 ms. The peer would hold the entry at ~225 ms. The same run
measured the tail drawn at 173, the nowview at 326 and the peer at 522. The views still go first, as
vividn ruled on #1607.

**Delivery.** The instance needs a way to wait for its next request. The probes in Chromium 153 found:

| option | needs | note |
|---|---|---|
| a reactor export the worker calls per request (`datum_line`) | nothing from the browser | no blocking: the instance lives between calls. The one that works everywhere |
| a loop blocking on `Atomics.wait` | cross-origin isolation, which the app already has (`crossOriginIsolated` true, `SharedArrayBuffer` present) | a second thread posts the request |
| a loop on JSPI (`WebAssembly.Suspending` present) | JSPI, which not every browser has | |

**What leaks between commands, and what must reset:**

- **The env the page sets per command** (`TZ`, `DATUM_CLIENT_ID`, `DATUM_DEVICE_KEY`, `LANG`,
  `FORCE_COLOR`, `DATUM_NO_DEFAULT_ANSWER`, `DATUM_SHOW`). It is set and cleared per request, as the
  loop does. All 29 lines exited as their processes did (code 0). Their output was not compared byte for
  byte with the processes'.
- **The frozen wall clock** (`DATUM_WEB_FROZEN_AT`). This is already a per-process `clock_time_get`
  override in `cli.ts`, so it becomes per call.
- **Thread-locals.** These hold a command's scope: color, width, ISO times, intent, the JSON echo, the
  duplicate no-op and the undo action. The REPL already resets them per line (da-3f9nl). That is the
  precedent, and the list to check.
- **A panic** is `abort` on wasm, so it traps and the instance is dead. Start a new one on the next
  request. The next open rolls back a hot journal, as it does today (da-dfmg8).
- **A migration** runs at the instance's first open. A deploy is a new module and so a new instance.
- **Another tab's write is the data-safety core.** wasm runs SQLite in `locking_mode = EXCLUSIVE`
  (da-o3sns), which trusts its page cache for the connection's life. That is right for a process that
  lives inside one acquisition, and wrong for a connection kept across a release. At each acquisition,
  the worker must compare the file's change counter (`changeCounter`, already read per process) with the
  one it released at. If the counter moved, the instance drops its `Db` and opens again (~3.7 ms) before
  running anything. Missing this means stale reads, and a write over pages that another tab changed.
- **The Web Lock is shared as today.** Holding the instance is not holding the files: the burst rule
  (release when the queue drains, or after 250 ms) stays, and the instance's file descriptors must point
  at whichever handles the next acquisition opens (`wasiFs.ts` builds them per process now).
- **Memory never shrinks.** One process's peak becomes the instance's for its life. The loop ended at
  **1,047 MiB**, because one `sync pipe verify` rehashed every row (below). Every other command stayed at
  3–4 MiB. The instance needs recycling past a bound.

## 3. Parallel reads

Which sync access handles two workers may hold on one file at once (Chromium 153, `measure-persist.mjs`
part 4):

| first | second | |
|---|---|---|
| `readwrite` | `readwrite` | refused (`NoModificationAllowedError`) |
| `readwrite` | `read-only` | refused |
| `read-only` | `readwrite` | refused |
| `read-only` | `read-only` | both held |
| `readwrite-unsafe` | `readwrite-unsafe` | both held |
| `readwrite-unsafe` | `read-only` | refused |

- **Reads can run beside reads, never beside a write.** `readwrite-unsafe` lets two writers in, but
  SQLite under WASI has no file locks (the `unix-none` VFS). Nor does it have WAL's shared memory. So that
  mode would need a locking protocol written for the shim.
- A second, read-only worker is possible.
- **The bound.** The tap's eight reads after its write took ~270 ms in sequence (excluding acquire).
  Their longest took ~64 ms. With eight free cores that saves at most ~200 ms at the last pane, and less
  on a phone's few big cores.
- **The costs:**
  - Each worker has its own instance, cache and memory.
  - Readers must open read-only, so `stats` cannot fill its cache.
  - A write waits until every reader has let go.
  - The `mode` option was probed only in Chromium.
- With a persistent instance the same eight reads take ~135 ms in sequence. Most of what parallelism
  would buy is the per-process set-up that the instance already removes. **Not filed.**

## 4. Fewer commands per tap

**Unchanged output.** Each read was run before and after the tap's write, at one frozen instant, with the
field flipped as in the real tap. Three printed byte for byte the same:

| command | alone |
|---|---|
| `fields --json` | 23 ms |
| `conflicts --json` | 14 ms |
| yesterday's `dayview --data-layer … --digests 0` | 69 ms |

The page's own rule is to refresh on a change (`changes.ts`), and that is already the `data_version`
key at the database's grain. Skipping more needs a finer key:

- `fields` and `conflicts` can only change with a spec, an alias, a kit or a new field, or with a
  conflict.
- The digest check (da-efxk5) draws a whole day to hash it. A per-day change stamp, kept by trigger as
  `stats_cache` keeps the days a write reached, could answer it without drawing.

**Batching a tap's refreshes into one process** would save the same fixed cost as the instance (≤ ~90
ms for nine processes). But it would hold every pane's draw until the batch ends, and lose the queue's
per-pane ranking and superseding (`key`, da-eoq3). The persistent instance saves this without either
cost. **Not filed.**

## Ranked

Desktop tap and its sessions at 290k: 567 ms of CLI time in all.

| | idea | measured upper bound | cost | risk | bead |
|---|---|---|---|---|---|
| 1 | one long-lived CLI instance per database worker | 368 ms (567 → 199) | large: a reactor entry in the CLI, `cli.ts`/`wasiFs.ts` for a long-lived instance, a reset on another tab's change, recycling | stale cache over another tab's write if the change-counter check is wrong; memory high-water; per-command state | da-jm4sv |
| 2 | one `Db::open` per process, not two | 79 ms (2.7 a process); native ~1.9 a command | small | low | da-z75fs |
| 3 | `--digests` from a per-day change stamp | ~50–70 ms per earlier day on screen | medium | a drawing that changes without data: key on the build too | da-7z5i9 |
| 4 | `Db::open` skips the every-open DDL when stamped | ~3.2 ms a process (~46 ms after 2) | small–medium | a dropped trigger no longer laid again | da-4qq8v |
| 5 | skip `fields`/`conflicts` when the write cannot change them | 37 ms (5 with 1) | small | a refresh missed | da-5n7ii |
| — | parallel read-only workers | ≤ ~200 ms at the last pane, with 8 free cores | large | as 3. above | not filed |
| — | one process for a tap's refreshes | ≤ ~90 ms | medium | as 4. above | not filed |

Ideas 2–5 apply without 1, and together save ~215 ms. With 1 in place, 2, 4 and 5 shrink to almost
nothing, and 3 stays worth ~38 ms.

## Also found

- **The first `sync pipe verify` on a new build rehashes every row at once.** It took 14.2 s, and the
  process's linear memory reached 1,047 MiB, at 290k entries.
  - `sync_row_hashes` is stamped by build (`crates/datum-core/src/db/sync.rs`, ~line 3300), and every
    deploy is a new build.
  - The worker runs one process at a time, so everything a tap asks waits behind it.
  - **da-vpwkw (P1).**
- **The first process on a freshly placed copy is slow, and not diagnosed.** The first `end walk`
  after the copy was written to OPFS took 1.6 s, all of it in dispatch. Its connection took 49 ms,
  against 3 normally. The runs after it took ~45 ms.
  - It is not the write's: with `--only fields`, `fields --json` was the first process instead and
    took 1.2 s (dispatch 1,136 ms; 8 ms warm).
  - Natively, on a warm OS cache, it is 10 ms.
  - It looks like a cold file, but that is not shown. Part 1's medians leave that run out (it is the
    worst run). Part 2 runs five writes before its timing begins.

## Reproduce

```sh
git apply web/scripts/measure-persist.patch
cargo build --release -p datum-cli && (cd web && pnpm run build)
cd web
node scripts/measure-log.mjs --layout desktop --presses 4 --json ../target/tap-desktop.json
node scripts/measure-persist.mjs --tap ../target/tap-desktop.json --runs 7 --json ../target/persist-desktop.json
cd .. && git apply -R web/scripts/measure-persist.patch
```

`--parts 1,2,3,4` runs a subset. `--only <text>` runs part 1 for the commands whose argv contains it,
and prints every run.
