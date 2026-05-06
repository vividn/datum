# daydumb retrospective — opal's perspective

Companion to `RETROSPECTIVE-daydumb-polecat.md` (quartz, general view) and
`RETROSPECTIVE-daydumb-garnet.md` (garnet, parse / render / sync wire).
Quartz covered cmd.rs/parse.rs, migration discipline, verb derivation.
Garnet covered datum-parse, the renderer's color rules, the conflict
queue, sync wire protocol. I won't repeat any of that.

Nine commits. The big ones, by area:

- **`datum-core/src/db/`** — split the 4204-LOC `db.rs` into 13 submodules
  (da-9l1.2, 48dc723).
- **`datum-sync-server`** — wire-protocol bridge, RFC 8628 device-code
  grant, `/v1/keys/wrapped`, library + binary split (da-6x9 phases 1–3,
  534f9f0); then ported the integration suite onto the real server in-process
  (da-6x9 phase 4, 7672c44).
- **`datum-sync-agent`** — the daemon: pure debounce + backoff +
  scheduler unit-tested without tokio, a tokio main loop on top, systemd
  user-service install (C4d / da-ndd.15, 72c0adc).
- **`datum now`** — track/ignore commands, `tracked_fields` +
  `field_state_colors` schema tables, ANSI truecolor renderer (da-z9e,
  8037ad9 — Sonnet 4.6 commit, but I own the scars).
- **Render wiring** — fixed `is_first_occurrence`, ported `datum tail`
  off the legacy column path, added `--table` (da-vkj, fbc83f6).
- **`stop --last-state` / `switch --moment`** + `find_open_block`
  helper (da-1mof, 2a78666).

So my beat: the database substrate, the sync backend, the sync daemon,
and the `now` surface. I have not touched parse.rs, the render color
algebra, dayview, or the kit system except as a downstream consumer.

## 1. Where I lived

`crates/datum-core/src/db/` is the address. After the split (da-9l1.2):

- `mod.rs` — 222 lines (Db lifecycle + re-exports)
- `insert.rs` — 976 lines (the single biggest survivor)
- `spec.rs` — 389
- `lookup.rs` — 282
- `types.rs` — 276
- ten others between 144 and 246
- `tests.rs` — 2564 lines (still `#[cfg(any())]`, see §4)

`crates/datum-sync-server/src/` — 11 files, ~1600 LOC total. Mostly mine
after da-6x9: `lib.rs` (530), `db.rs` (281), `device_code.rs` (201),
`auth.rs` (153), `keys.rs` (41), `models.rs`, `sync.rs`.

`crates/datum-sync-agent/src/` — 10 files, ~1665 LOC. All mine: the pure
core (`debounce.rs` 134, `backoff.rs` 159, `scheduler.rs` 289) and the
tokio wrapper (`main.rs` 368, `notify.rs` 216, `liveness.rs` 187).

The renderer wiring touches in da-vkj live in `commands/cmd.rs::cmd_tail`
and `render/mod.rs::show_action_with_context`. The `now` view code is in
`cmd.rs::cmd_now` (~90 LOC) plus `render/mod.rs::print_now_view` and
friends (~150 LOC).

## 2. What was harder than expected

### The db.rs split was 80% mechanical, 20% nasty visibility decisions

I expected `git mv db.rs db/mod.rs && cargo fmt`. The reality:

- About thirty external call sites use `db.conn()` to grab the underlying
  `rusqlite::Connection` for ad-hoc queries (datum-sync-client,
  datum-commands, internal modules). The bead asked to make `conn()`
  `pub(crate)`. I refused: the bead's *other* explicit acceptance
  criterion was "public API of Db is unchanged", and demoting `conn()`
  breaks both contracts simultaneously. Whoever wrote that bead hadn't
  grepped. **Lesson for future polecats: when bead acceptance criteria
  conflict, document the conflict in the commit message and ship the
  conservative move.**
- `detect_for_entry` was supposed to take `&Db`, per the bead. It can't.
  It runs *inside* `insert_entry_in_tx` with the caller's transaction.
  If it took `&Db` it would have to grab a fresh connection — and the
  conflict row would no longer be atomic with the triggering insert.
  This is not visible from the bead's vantage point; only visible if
  you've already mapped the call graph.
- `pub(super)` vs `pub(crate)` for cross-submodule helpers
  (`time_expr`, `filtered_where`, `like_prefix_pattern`, `fetch_entry_any`)
  — every helper had to be classified by whether it's reachable from
  sibling submodules only or also from `migrations.rs`'s `try_insert`
  callsite. There's no compiler hint for "you over-permissioned this";
  it's all eyeballed.

The split itself is the right move. The cost is: `cargo build -p datum-core`
still recompiles every db submodule together (they all `pub use`-roundtrip
through `mod.rs`), so the incremental win is smaller than file count
suggests. The grep win is bigger than the build win.

### The integration-test-port to a real server was where the bodies were buried

When the sync stub (a hand-rolled axum app in `tests/sync_support`) got
replaced with the real `datum-sync-server` library spawned in-process,
ten or so subtle assumptions surfaced:

- The old stub minted token strings like `"stub-token-{n}"`. Tests
  asserted on those strings. The real server mints JWTs. I had to relax
  ~12 assertions from "exact token == X" to "token is non-empty and
  shape is `head.payload.signature`". That's correct, but every test
  reads a little less informatively now.
- Login event fields: the stub's response returned a username string;
  the real server returns the canonical UUID per protocol spec. One test
  was comparing username, which had been wrong all along — only visible
  once it broke.
- Device-code flow: the stub's auto-authorizer marked the code as
  authorized on the *first* poll. The real server's poll path needs a
  separate POST to `/authorize` first. I had to keep a polling thread
  alive in the test fixture that mimics the second-poll-marks behavior
  to preserve the pending → authorized transition tests.

The takeaway: **stubs that diverge from the spec encode bugs in tests
that only become visible when the stub is removed.** I'd rather have
seen this earlier — the wire spec was locked in da-6hn; the stub should
have been generated from the same source.

### The sync daemon's pure core was easy. The wrapper was where the time went.

`debounce.rs`, `backoff.rs`, `scheduler.rs` are pure state machines that
take `Instant` as input. Total writing time: an evening. Total tests:
seven, all sub-millisecond.

Then the tokio wrapper (`main.rs`): spawn one worker per DB, each owning
a `mpsc::Receiver<Instant>` for write-debounce ticks plus a
`watch::Receiver<()>` for shutdown, all running through `spawn_blocking`
because `reqwest::blocking::Client` and `rusqlite::Connection` are
`!Send` over async boundaries. Then the `notify` crate's filesystem
watcher feeds the channels. Then the systemd plist generator. Then the
SIGTERM graceful drain that has to JoinSet-await every worker.

The acceptance tests in the bead (`two_instance_daemon_sync_converges`
and `daemon_resumes_after_relay_downtime`) are scaffolded `#[ignore]`d
because they need a live relay + two XDG roots. I shipped them that way
intentionally — they document the acceptance contract and the setup
required, but CI never runs them. **The pure core is well-tested; the
binary's loop has zero integration coverage.** This is the gap I'd
close first if I were back here (see §5).

### `is_first_occurrence` was wrong for boring reasons

The bug: every `datum switch` was emitting the "first tracked" marker
(✨ / ∅) regardless of whether the field had history. Cause: the probe
called `db.load_spec(field).is_none()`, but `ensure_tracked()` only
creates a spec for `Occur` and `Start`. A switch on a field that had
never been occured/started looked spec-absent → first-tracked → ✨ on
every single call. The fix is one line (probe entry count, not spec
presence). The cost was an afternoon of staring at the renderer
suspecting it was a renderer bug. **It wasn't.** It was a contract
mismatch between two helpers in different files.

This is the same shape as the bugs garnet's renderer keeps absorbing:
the renderer is correct locally; some upstream feeder is wrong. In my
case the feeder was three crates away.

## 3. What was easier than expected

### Splitting the pure core out of the agent paid off immediately

`scheduler.rs` is a state machine — `Scheduler::poll(now: Instant) →
Action`. No clock, no I/O, no async. Tests assert "after 3 failures the
next retry is at T+80s" without sleeping. I wrote seven tests in 30
minutes and got every behavior the bead specified pinned down.

The same pattern works for `Debouncer`, for `Backoff`, would work for
the per-direction state in the binary loop too. **Anywhere with a
clock, separate `now()` from the policy.** This is the cheapest
testability win in the codebase, and I think the rest of daydumb (the
timing/retry logic in datum-sync-client especially) would benefit from
the same treatment.

### Making `datum-sync-server` a library cost me one `Cargo.toml` change

The old layout was a `main.rs` binary. Phase 4 of da-6x9 needed to spawn
the real app from inside an integration test in a sibling crate. The
fix: split `[lib]` and `[[bin]]` in `Cargo.toml`, move `build_app()` and
`AppState::new()` into `lib.rs`, leave a four-line `main.rs` that calls
into the lib. `cargo build` exit 0 first try.

The reason this was so cheap is that the existing code already routed
everything through an `AppState` struct rather than module globals.
That's a quiet architectural win that paid off on a refactor I bet
nobody planned for.

### `count_entries_before` made the tail rebuild path tractable

`datum tail`'s switch from the legacy column view to the per-row info
line needed to reconstruct the per-row `RenderContext` for each
historical entry — including whether *that row, at the time it was
written* was the first occurrence. Adding `Db::count_entries_before(field,
t) → usize` made it a one-liner per row. It's O(N²) over a long tail
which I'd revisit if anyone ever ran `tail -n 100000`, but for the
default `tail -n 50` it's invisible.

### The migration framework absorbed `tracked_fields` + `field_state_colors` without drama

I added two tables in da-z9e (m19_to_20). The framework just swallowed
it: write the up + down + roundtrip test, register, done. This is the
piece quartz called out and they're right — but from the user side
(adding a migration, not maintaining the framework), the discipline
shows up as "I literally can't break the schema without the chain
test catching it". That's rare.

## 4. Bugs I suspect or noticed but didn't fix

1. **`datum-sync-server/src/device_code.rs::random_token` and
   `random_user_code` use system-clock nanoseconds, not real entropy.**
   The comment justifies this — "device_code is single-use,
   short-lived, requires a separate /authorize POST". That's fine for
   user_code (6-digit display string, the user types it). It is **not**
   fine for `device_code`, which is a bearer credential; an attacker
   who can predict a victim's device_code can race the /authorize step.
   Pull `rand` in or use the same JWT-secret-keyed source we use for
   tokens.

2. **`datum-sync-agent` creates a fresh `reqwest::blocking::Client` per
   push and per pull** (`main.rs::do_push`/`do_pull`). Each call pays
   the TLS handshake. For the 30s pull cadence on N DBs that's N TLS
   sessions per 30s forever. Cache the client per worker.

3. **`db/insert.rs::detect_for_entry` runs inside the same transaction
   as the entry insert (good)**, but **`auto_track_field` calls
   `ensure_tracked` only for `Verb::Occur` and `Verb::Start`** — not
   switch, not stop, not note. A user whose first interaction with a
   field is `datum switch mood happy` has no tracked_fields row. They
   discover this later: the `datum now` view doesn't list the field
   until they `datum track` it manually. Decide whether that's
   intended; if not, broaden the auto-track to any non-note verb.

4. **`db/mod.rs::migrate()` runs `schema.sql` twice with hand-patches in
   between** (quartz called this out — but now that the split has
   landed and `mod.rs` is only 222 lines, the cleanup is finally
   mechanically easy. The new ALTER columns from da-z9e
   (`tracked_fields`, `field_state_colors`) live in `schema.sql` only,
   not in the pre-flight ALTERs, and they don't break anything —
   suggesting the pre-flight set could be dropped now).

5. **`db/tests.rs` (2564 lines) is `#[cfg(any())]`** — I moved this file
   during the split and left the gate in place because the bead scope
   was "split, don't rewrite". Quartz also called this out. The split
   was the moment to either re-enable or delete it. I picked neither.
   **That's a regret.**

6. **`force_recreate_schema` is `pub`** on `Db`, but the only caller is
   `cmd_db_purge` inside `datum-commands`. Should be `pub(crate)` —
   except (see §2) that demotion is the kind that fights cross-crate
   re-exports. Re-evaluate post-v1 lock.

7. **`commands/cmd.rs::cmd_now` reads `pending_conflict_count()` and
   silently swallows errors**, which I wrote that way intentionally
   ("the now view should never block on a conflict-count read") — but
   it would also swallow a *legitimate* "conflicts table doesn't
   exist" symptom on a misconfigured DB and you'd never see it. At
   minimum, log to stderr at debug level.

## 5. The non-obvious improvement (the only section that matters)

> **Push the agent's "outer loop" through the same pure-core
> discipline as the scheduler. Today the binary's tokio dance is the
> single largest piece of untested code in the workspace.**

Right now the agent looks like this:

```
debounce.rs       — pure state machine, 7 tests, 0 tokio
backoff.rs        — pure state machine, 4 tests, 0 tokio
scheduler.rs      — pure composition of the above, 5 tests, 0 tokio
main.rs (368 LOC) — tokio, mpsc, watch, JoinSet, spawn_blocking,
                    notify, signal handling, pid file —— ZERO TESTS
```

The bead's two integration tests
(`two_instance_daemon_sync_converges_without_manual_command`,
`daemon_resumes_after_relay_downtime`) are `#[ignore]`d because they
need a live relay + two XDG roots. CI doesn't run them. The acceptance
contract for the daemon is documented in test bodies that nothing exercises.

The fix is not "write more integration tests against a real relay".
The fix is to extract a `Driver` type with this shape:

```rust
pub struct Driver<C: Clock, T: Transport, F: FsEvents> {
    schedulers: HashMap<DbId, Scheduler>,
    clock: C,
    transport: T,
    fs_events: F,
}

impl<C: Clock, T: Transport, F: FsEvents> Driver<C, T, F> {
    /// Drive one tick: drain fs_events, poll each scheduler, dispatch
    /// any push/pull through transport, fold results back. Returns the
    /// time at which the caller should next call tick().
    pub fn tick(&mut self) -> Instant { ... }
}
```

Then `main.rs` becomes ~50 lines: build a `Driver<RealClock,
ReqwestTransport, NotifyEvents>`, loop calling `tick()` and sleeping
until the returned instant. The integration acceptance becomes a
`Driver<TestClock, FakeTransport, ScriptedEvents>` test that exercises
the same code path as production *without* a live server, in
microseconds.

This is exactly the layering that worked for the scheduler — and the
reason it didn't ship in C4d is that I was racing against the bead's
acceptance criteria, which were specified per-component (debounce,
backoff, scheduler) and not for the combined driver. The pure-core
discipline halts at the boundary the bead names.

The companion move: do the same for `datum-sync-client`. That crate has
its own retry / device-code-poll / token-refresh state machine inside
HTTP-blocking calls, and its tests are all stubbed-server-against-real-client.
Extract a transport-agnostic `Session` state machine and the same
fake-transport tests become millisecond-cheap.

Garnet wouldn't have noticed this because they wrote the wire protocol
and the client's outer surface, not the daemon's runtime layer. Quartz
wouldn't have noticed because it doesn't manifest as a visible bug —
it's invisible coverage debt. Whoever writes `datum-sync-agent.daemon-mode-2`
in six months will rediscover it the hard way.

## 6. Relationship to the whole

`datum-core/src/db/` is the foundation. Every other crate ultimately
calls into `Db`. The split made it look like a directory of leaves, but
it's a directory of methods on one type — `mod.rs` re-exports
everything and the public API is `Db::*`. From outside the crate, the
split is invisible. Inside the crate, it's the difference between
"open the file" and "find the right submodule". For me, in the middle
of a refactor, that's the difference between feasible and infeasible.

The sync server and the sync agent are orthogonal: they don't share
code, they share a wire spec. The relay is content-blind, the agent
holds the keys. That separation is the whole architectural point of
the sync system, and from the inside it shows: I can change one without
touching the other. `datum-sync-protocol` is the contract between
them and it's a tiny crate, which is correct.

The `now` view straddles `datum-commands` (orchestration), `datum-core`
(the `tracked_fields` schema + `latest_entry_for_now`), and the
renderer (`render::print_now_view`). It's currently a clean three-tier
shape — but the temptation in da-z9e was to put more logic in
`cmd_now` and less in the renderer. I held the line. If the
"active/stopped/ignored" sectioning ever needs to be reused (e.g., a
web nowview), the split will pay off.

The piece I'd call out as **most fragile** is the agent's blocking ↔
async boundary. Every push/pull is `tokio::task::spawn_blocking → reqwest::blocking::Client →
rusqlite`. There's no async path through the stack at all. The reason
is principled (the SQLite connection is held by `Db` and routes
everything through one connection; async SQLite would need a connection
pool, which conflicts with WAL semantics) — but it means the agent can
never natively use HTTP/2 multiplexing or `tokio-rusqlite`. If the
relay ever becomes high-fanout, that's the constraint that bites first.

Three things I'd do, in order, before v1 lock:

1. Extract the agent's `Driver` and write the in-memory integration
   tests (§5). This is the one move that would pay back the most,
   measured in future-polecat hours saved.
2. Either re-enable or delete `db/tests.rs` (§4.5). Cold storage rots,
   and the split was the moment to decide.
3. Replace `device_code.rs::random_token`'s nanos-based generation with
   real entropy (§4.1). It's a five-line fix and a real auth gap.

If I had a free week without a backlog, I'd do #1 and use the
infrastructure to write a chaos test that randomly drops fs events,
fails pushes, and asserts that the agent eventually converges. The
scheduler's correctness is provably right at the unit level; the
binary's correctness is a vibe.

— opal, 2026-05-06
