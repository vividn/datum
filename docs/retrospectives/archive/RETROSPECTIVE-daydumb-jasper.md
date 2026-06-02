# RETROSPECTIVE — daydumb from the render/import/web seat

**Author:** polecat jasper (rig: daydumb)
**Date:** 2026-05-06
**Workspace HEAD:** `polecat/jasper/da-gm2h@motnu5zg`, off `main` at `374740f`
**Scope:** the parts of the workspace I've actually spent time in — the visualization stack (`dayview`/`nowview`/`tail` rendering), the legacy `db import` path, the EntryHistory/undo machinery, the SvelteKit PWA, and the testing harness for time and shell snippets. Quartz wrote the general retro (`RETROSPECTIVE-daydumb-polecat.md`); this one only adds what I can see from where I sit. 38 commits to `origin/main`.

---

## 1. Where my hours went

By rough commit count against `git log --author=jasper origin/main`:

| Area | Commits | Headline beads |
|---|---|---|
| Render / dayview / nowview / tail | 12 | da-jyi (nowview terminal), da-vp7 (multi-day SVG/PNG), da-6jy (nowview reuses dayview path), da-w6a (--watch refresh), da-tbe (axis direction), da-dqb (FieldSpec y/height/z_index), da-uuu (per-field render cache), da-25l, da-yiy, da-r1q, da-gl22, da-2zd |
| Legacy `db import` path | 6 | da-5ns.6 (v=20 schema map), da-6l0 (bulk fast path), da-8p1 (prepared-statement cache), da-1el (modify_time + `_import` marker), da-9u9.1 (escape hatch), da-fab (rename `migrate import` → `db import`) |
| Test infra | 4 | da-5ss (`set_time`/`advance_time`), da-nkk (README bash-snippet runner), da-tly (slug-collision flake fix), da-r1q (regression test) |
| EntryHistory + undo | 3 | da-gnj.4 (D4 history table + `datum history`/`datum why`), da-p805 (soft-delete tombstone undo), da-jsmr (template interpolation) |
| Sync extraction | 2 | da-6hn.1 (extract `datum-sync-protocol`), da-6hn.2 (lift `sync_once`) |
| Kit slices | 3 | da-d4t.2/.3/.7 (relative_to anchors, aliases-in-TOML, chores kit) |
| Args refactors | 2 | da-e2e (Phase A: `LogArgs` composition), da-6qj (Phase C: mutate-command `OutputArgs`) |
| Web PWA bootstrapping | 1 | da-b6g — `web/` (SvelteKit + SQLite WASM + OPFS + service worker) |

The rest are one-offs (parser port da-sw2, switch-cmd parity da-4pne, S12 array-state comma-split, etc.).

The single file I have re-opened the most this quarter is **`crates/datum-commands/src/render/mod.rs` (1400 LOC)** plus its sibling **`render/action.rs` (746 LOC)**. The next is **`crates/datum-commands/src/cmd.rs::cmd_db_import`** at line 3326 onwards — the import path is a 200-line linear pass inside the cmd.rs mega-file. The third is **`crates/datum-core/src/field_spec.rs` (1674 LOC)**, where every render-related spec column landed (y/height/z_index for dayview; tags; render-visibility map).

---

## 2. What was harder than expected

### The dayview/nowview "shared render path" is not actually shared
da-6jy was filed as "nowview should reuse dayview's render path". I closed it and wrote tests. But under the hood, the two commands still build their own context. Compare:

- `crates/datum-commands/src/commands/dayview.rs` — 1521 LOC
- `crates/datum-commands/src/commands/nowview.rs` — 1166 LOC
- `crates/datum-layout/src/dayview.rs` — 1013 LOC, with **`dayview_svg.rs`** at 670
- `crates/datum-layout/src/nowview.rs` — 212 LOC, **`nowview_svg.rs`** at 484, **`nowview_terminal.rs`** at 554

The "reuse" in da-6jy was reusing the **renderer leaf** (state-region drawing primitives) but not the **plan**: lane assignment, time-window slicing, color resolution, spec lookup. Each of those happens twice (three times for nowview because it has three surfaces: terminal, SVG, PNG-via-SVG). When I added `tail`'s relative-day prefix (da-n1q), I had to thread the same logic through render/mod.rs's action path. There are now three spec-resolution paths for what is conceptually one operation: "given this entry list and these field specs, where does each glyph go and what color is it?"

### `dayview --watch` and `nowview --watch` are duct-taped onto SQLite
`crates/datum-commands/src/commands/watch.rs` (143 LOC, da-w6a) polls `PRAGMA data_version` every 500ms. SQLite's `data_version` only ticks for **the calling connection's view of the WAL**; it works for cross-connection writes on the same `Db` because we hit the same WAL, but it is **not a notification mechanism** and it does not generalize to filesystem-mediated changes (e.g., another process's `cp` over the DB file). The doc-comment in `watch.rs:23` is honest about that. It works *now* because there is exactly one writer (the CLI). When the sync agent lands and starts writing in the background, refresh latency will spike to the 60-second periodic floor in the worst case, and I do not know how I would notice that regression — no test exists that asserts on cross-process write→render lag.

### `db import` bulk path is a separate world from `try_insert`
da-6l0 added `Db::insert_entry_bulk_in_tx` (`crates/datum-core/src/db/insert.rs:552`). It bypasses:
- `try_insert` slug collision retry
- `derive_verb_for_storage` (canonical state derivation)
- `EntryHistory::record` (no `user_edit` row)
- `OnConflict` strategy resolution

That bypass is **the entire point** — it is why a 100k-row backup imports in seconds now instead of three minutes (da-8p1). But it means imported entries are second-class: `datum history <imported-hid>` shows nothing, `datum undo` cannot reach them via the regular path, and any future feature that assumes "every entry has a creation history row" will silently fail for legacy data. da-1el added a structured `_import` marker in `extra_data` to compensate, which is fine, but the schema doesn't enforce that the import path **must** write that marker. A future patch to bulk_insert could drop it and nothing would break in tests.

### Field-spec render-visibility (da-2zd) is `bool | { surface: bool }` and that polymorphism leaked
The column stores either `true`/`false` or `{"tail": true, "dayview": false}`. Serialization round-trips; semantics don't always. The CLI flag `--render bool` and `--render tail=false,dayview=true` parse to two different shapes that converge in storage but diverge again at read time, where every callsite has to handle "bare bool" vs "map" via `match`. `field_spec.rs::ResolvedRender` papers over it but every consumer in `render/mod.rs` has to remember to call the resolver.

### EntryHistory has no compaction story
da-gnj.4 shipped a `entry_history` table that grows linearly with edits. Every `datum edit` writes a row (good — that's the audit trail). Every `datum undo` writes a row too (also good — undo is itself an edit). There is no `--prune-history` knob, no max-versions cap, no test for "10k edits to one entry." `datum history --limit` exists and is what the user sees; the table itself just grows. Sync replicates the rows verbatim. For a data-as-art personal tracker this is fine; for a power user logging mood-on-a-watch every minute, it isn't. I did not file this; I am filing it now (§4).

### "Phase A and Phase C" args composition refactor is asymmetric
da-e2e was Phase A (LogArgs got composed `TimeArgs`/`DataArgs`/`OutputArgs`/`MergeArgs` sub-structs publicized). da-6qj was Phase C (mutate-command `OutputArgs`). Phase B never landed under my name and I could not find it in `bd list --status=closed`. The args layer therefore has two regions where composition is real (LogArgs, mutate args) and an unmodified middle. `crates/datum-commands/src/args.rs` reflects this — half the file is composed structs, half is freestanding flat `*Args`.

---

## 3. What was easier than expected

### Test clock injection via `-t` parser reuse (da-5ss)
`crates/datum-core/src/test_util.rs::set_time(s) -> TimeGuard` (line 388) installs a thread-local `MockClock` parsed by **the same `parse_time_arg` the CLI uses for `-t`**. That single decision — reuse the user-facing parser for test time — meant I could write `set_time("2026-04-01T09:00Z")` or `set_time("9:00am")` or `set_time("0900")` and they all work. `advance_time("+5m")` reuses `parse_duration` the same way. After this landed I retired ~6 test files that had been hand-rolling clock fakes. No bug since.

### Slug-collision flake fix (da-tly) was a one-line test-side fix
The flake was: two `now_cmd` tests in the same second produced the same slug, so the second insert errored under the slug-uniqueness contract. The fix was not in the database — it was using distinct timestamps in the test setup. The slug contract was correct; the tests were lazy. I'd expected a layered conflict-resolution refactor; it took a 4-line patch.

### Bash-snippet runner (da-nkk)
`crates/datum-cli/tests/readme_snippets.rs` (420 LOC) extracts every fenced ```bash block from `README.md`, runs each in a tempdir against the freshly-built `datum` binary, and asserts exit code 0. Took an afternoon. Caught two stale flag references the same week.

### Web PWA (da-b6g) — SvelteKit static + SQLite WASM + OPFS just worked
`web/` is 912 LOC total. The whole "PWA with offline storage that uses the same SQL schema as the CLI" feature is **smaller than `cmd.rs::cmd_db_import` alone**. `web/src/lib/db.ts` is 229 lines, `web/src/lib/parse.ts` is 133, `web/src/routes/+page.svelte` is 261. The service worker is 51 lines. SvelteKit static-adapter + `@sqlite.org/sqlite-wasm` + OPFS is a remarkably short path to "offline-capable SQLite app in a browser." It has no sync — that's da-636's job — but the data plane works.

### Migration escape hatch (da-9u9.1)
`datum db migrate --force-recreate` deletes and re-runs schema.sql + migrations from scratch. One flag. Saved me ~3 debugging sessions on the 2026-04-30 `kit_id` failure. The lesson: when migrations are append-only and total, always ship a "burn it and rebuild" knob alongside.

### Args refactors (Phase A/C) were boring in a good way
da-e2e and da-6qj. The bulk of each PR was mechanical search-and-replace. The composed sub-structs (`OutputArgs`, `TimeArgs`, etc.) had been designed correctly the first time; promoting their visibility was a non-event. Twice in a row I have done a "publicize composed args" refactor and twice in a row Rust's borrow checker has caught the only thing I would have gotten wrong (a stale `&mut` on the parent struct). The type system is doing its job here.

---

## 4. Bugs I suspect or noticed but wasn't asked to fix

File-level, no apologies, file as beads:

1. **`watch_loop` fails silently on cross-process writes**
   `crates/datum-commands/src/commands/watch.rs::data_version` reads `PRAGMA data_version` on the calling connection. SQLite's docs say this counter "should be considered unreliable for cross-process change detection." The 60s periodic floor masks the problem. Filing.

2. **Bulk-import path skips EntryHistory rows**
   `crates/datum-core/src/db/insert.rs::insert_entry_bulk_in_tx` (line 552) does not call `history::record`. After `datum db import backup.json`, `datum history <hid>` returns empty for every imported entry. Either bulk-insert should write an `_import` history row, or `datum history` should synthesize one from the `_import` marker in `extra_data`.

3. **`field_spec.rs` is 1674 LOC and growing**
   It now holds: spec storage shape, `DerivedAlias` lifecycle, render-visibility polymorphism, y/height/z_index resolution, `interpolate_template` for slug formatting, `ResolvedRender`, `ResolvedDayviewLayout`. Quartz noted cmd.rs and parse.rs as the mega-files; field_spec.rs is the third one and it crept in under my watch (da-a7g, da-2zd, da-dqb all added columns to it).

4. **`%field%`/`%state%`/`%uid%` template interpolation does not escape delimiters**
   `crates/datum-core/src/field_spec.rs::interpolate` (line 834). If a field name contains `:` and the slug template is `"%field%:%state%"`, the resulting slug has two colons and the slug→hid resolver can't disambiguate. There's a sanitize layer for control chars but not for the delimiter itself. Add a test: field named `"a:b"`, slug template `"%field%:%state%"`.

5. **Three nowview surfaces, three context builds**
   `nowview.rs` (1166 in commands) + `nowview.rs` (212 in layout) + `nowview_svg.rs` (484) + `nowview_terminal.rs` (554) — each surface re-fetches specs and re-resolves colors. da-jyi added the terminal surface and copied the SVG context-build verbatim because lifting it was out of scope.

6. **Web schema vs. CLI schema have no shared migration**
   `web/src/lib/db.ts` defines its own `CREATE TABLE` statements. The CLI runs `crates/datum-core/src/schema.sql`. They drifted after da-a7g (tags column added to CLI, not to web). When sync lands the web client will be a peer; today it's an incomplete schema.

7. **`set_time` is thread-local; parallel tests sharing a Db do not see the mock clock**
   `test_util.rs::set_time` installs the clock on the calling thread only. If a test spawns a worker thread (e.g., the watch loop test, or any sync-agent test), inserts there go through `DatumTime::now()` which reads the *worker's* (wall) clock. The doc-comment in `set_time` does not warn about this. I have at least one test (`watch_loop` test in `watch.rs:138`) that works by accident.

8. **`db_import` `dur_parse_warnings` is printed to stderr, not collected**
   `cmd.rs:3393` `eprintln!` per malformed dur. On a 100k-row backup with a few hundred bad rows you get a wall of warnings. They should aggregate to a single summary line, or write to a sidecar `<backup>.import.log`.

9. **`field_spec.rs::ResolvedDayviewLayout` collapses missing y/height to defaults silently**
   da-dqb. If two fields have `y=None`, both get y=0 and overlap in dayview. There is no warning. The user only notices when they screenshot dayview and see one field hidden under another.

10. **EntryHistory has no compaction**
    `crates/datum-core/src/history.rs` (262 LOC). Linear growth per edit. No `--prune` command, no max-versions cap, no per-entry rollup. Will bite power users.

11. **`commands/db.rs` is 88 lines of dispatch; the implementations are in `cmd.rs`**
    `db.rs:23-24` calls back into `crate::cmd::cmd_migrate` and `crate::cmd::cmd_db_import`. Same R2-stub pattern quartz called out, applied to db subcommands. `cmd_db_import` should live in `commands/db.rs` and stop counting against `cmd.rs`'s 5498 LOC budget.

---

## 5. One non-obvious improvement: lift a single `RenderPlan` over dayview/nowview/tail

This is the section I most want the next polecat to read.

### The observation

`tail`, `dayview` (multi-day SVG/PNG), and `nowview` (terminal/SVG/PNG) are conceptually **the same query** at different time scales:

> Given a time window `[t0, t1]`, a set of fields, and the current spec state, lay out every entry into colored regions on a 2D grid (time on one axis, lane on the other), with state/duration semantics applied.

`tail` collapses the grid to one row per entry. `nowview` maps the last 30 minutes onto a single horizontal band. `dayview` maps a day onto a wide grid with multi-day vertical stacking.

Today these are three separate pipelines that **each** rebuild:
- entry list fetch (`Db::list_entries` with overlapping time predicates)
- spec resolution (`field_spec::resolve`)
- color resolution (read in by da-bop/da-8mu/da-438y, still smeared across `render/colors.rs` and `cmd.rs`)
- lane assignment (y/height/z_index — only dayview/nowview do this)
- state region computation (`render/action.rs::resolve_state_regions`)

Combined LOC across the three: roughly **5,620 lines** (the seven files in §2 above).

### The improvement

Define a single trait/struct in `datum-layout`:

```rust
pub struct RenderPlan {
    pub window: TimeRange,
    pub lanes: Vec<Lane>,           // y/height/z_index resolved
    pub regions: Vec<StateRegion>,  // start, end, color, state, field
    pub markers: Vec<Marker>,       // point-in-time entries
    pub spec_index: SpecIndex,      // for legend / hover labels
}

pub fn build_render_plan(db: &Db, q: &PlanQuery) -> Result<RenderPlan>;
```

Then:
- `commands/dayview.rs` becomes ~200 lines: build `PlanQuery`, call `build_render_plan`, hand the plan to `layout::dayview_svg::render` or `layout::dayview::render_terminal`.
- `commands/nowview.rs` becomes ~150 lines: same shape, narrower window, hands plan to `nowview_{terminal,svg}::render`.
- `commands/tail.rs` becomes ~250 lines: same plan, collapsed to one row per region.
- The three `*_svg.rs` files stay, but they take `&RenderPlan` and emit pixels — no DB access, no spec resolution.

This is **not** a big-bang refactor. The shape of `StateRegion` already exists in `render/action.rs::ResolvedAction`. The shape of `Lane` already exists in `field_spec::ResolvedDayviewLayout`. The lift is: **make those types canonical, move them to `datum-layout`, and rewrite the three command modules to consume them instead of recomputing them.**

### Why quartz won't propose this

Because they're working from the parser/cmd.rs side. They see the dispatch (`commands/dayview.rs` calls into `cmd::cmd_dayview` or directly into the layout crate) and they see the duplication call out from a structural skim. They don't see that the **shape of the duplication** is "everyone computes the same five-step pipeline." That's only obvious if you've added a feature to one and had to port it to the other two — da-jyi (added a terminal surface), da-vp7 (added multi-day stacking), da-6jy (tried to share). Three times now I have copied this pipeline.

### Order of operations

1. Define `RenderPlan`/`PlanQuery`/`Lane`/`StateRegion` in `datum-layout/src/plan.rs` (no behavior; pure types).
2. Move the existing `ResolvedAction`/`ResolvedRender`/`ResolvedDayviewLayout` types so they ARE these types (rename, don't duplicate).
3. Move spec + color resolution into `build_render_plan`. Keep current command-level functions calling into it.
4. One command at a time: dayview → nowview → tail. Verify visual goldens (which we don't have — see quartz §3 — add some).
5. Delete the now-dead duplicate paths.

Cost estimate: 3-4 polecat-weeks. Reward: the next "render feature" (legend rendering, density coloring, per-spec animation) lands once, in three surfaces, automatically.

---

## 6. My area's relationship to the whole

### The render/layout layer is a leaf consumer that is leaking up the spec column

The visualization stack is supposed to be a **leaf**: it pulls from `Db::list_entries` + `Db::get_spec`, decides where to draw, and exits. That's clean. The leak is that `FieldSpec` (in `datum-core`) now carries view-presentation columns:
- `y: Option<f64>` (line 135)
- `height: Option<f64>` (line 141)
- `z_index: Option<f64>` (line 148)
- `render_json` (visibility per-surface map, da-2zd)

These are **pure renderer concerns** living in the data model. Adding them was expedient — the user wants to type `datum spec mood --y=0.4 --height=0.2` and have dayview pick that up — but they couple every CLI command that touches specs to renderer semantics. A spec-export tool needs to know about y/height. A sync agent has to replicate them. The migration table got a column for them.

The right model: `FieldSpec` stays a pure user-intent record. A separate `dayview_layout` table (or a JSON blob in a `presentation` column) holds renderer concerns. Spec sync replicates the intent; presentation is per-device or shared via the kit format.

### The import path is a one-way membrane and a useful one

`db import` reads CouchDB JSON, writes Entries. It does **not** read from anywhere else and nothing reads back into it. Refactoring around it is safe — the surface is `(file path, dry_run) → side effect`. The internal mess (da-5ns.6's dur table, da-1el's modify_time preservation, da-7sr's verb-derivation timing fix) is contained. Whatever happens to `cmd.rs` in the v1 carve-up, the import logic should move to `commands/db.rs` as one ~600-line module and stay there.

### EntryHistory + undo is a coupling hub I underestimated

`history::record` (`crates/datum-core/src/history.rs:110`) is called from `try_insert`, `update_entry`, `soft_delete_entry`, `undelete_entry`, `tombstone.rs::undo`. Five call sites, all in `db/`. That's the right ratio. But it is also an **append-only log of every mutation** that sync now has to replicate, that the web app has to mirror, and that future features (audit, "why did this entry change", legal export) will all hang from. It's gone from a feature to a foundation in two months. It needs a compaction story before v1, and the web schema needs to mirror it before sync lands.

### Web is decoupled and should stay that way

`web/` does not import from `crates/`. It re-implements the entry shape in `web/src/lib/types.ts` (36 lines), the parser in `parse.ts` (133), the database in `db.ts` (229). This is a maintenance tax — schema changes happen in two places — and the right answer is **not** wasm-compiling `datum-core` for the browser (binary size, build complexity). The right answer is a code-generation step from `schema.sql` → `web/src/lib/schema.generated.ts`. Today there is no such step. Adding one is a half-day. Without it the web schema **will** drift further (it already has — `tags` from da-a7g is missing).

### Test infra is a force multiplier I keep undervaluing in scope estimates

Every time I have spent a day on test infra (da-5ss `set_time`, da-nkk snippet runner, da-tly slug-collision fix), the ROI has been "next bug took 30 minutes instead of a day." The bash-snippet runner has caught two stale README references already. The deterministic clock retired six bespoke clock fakes. **Quartz is right that the codebase is missing a property-test harness; I'd add: it is also missing visual goldens for SVG/PNG output and a cross-process write→render-refresh latency test.** Both belong in the same test-infra investment.

---

## Closing — the rendering polecat's view

The visualization layer is the most copy-pasted region of the codebase. It is also the one that delights users when it works — `datum dayview` is the feature that makes daydumb feel like data-as-art. Investing the 3-4 weeks to lift `RenderPlan` is the highest-leverage refactor outside the cmd.rs/parse.rs carve-up that quartz has already named.

The legacy `db import` path is small, contained, and ready to move out of cmd.rs. Do that first; it's a free win and it shrinks the mega-file.

The web app is the most underestimated part of the project. 912 LOC, offline-capable, same SQL semantics. When sync lands it becomes the second peer in the network. Schema codegen to `web/` should land before the first sync release, not after.

EntryHistory is going to need a compaction story before v1. File a bead, scope it, do it.

— jasper, 2026-05-06

---

## Status (as of 2026-06-02): Closed

- **§5 RenderPlan lift over dayview/nowview/tail** → **open**, tracked as **`da-5895`** epic ("Daydumb render-layer unification") combined with ruby's parallel proposal. 5 sub-beads planned. The shape of the refactor is documented; nobody has picked it up yet.
- **§2 dayview/nowview "shared render path" not actually shared** → **open**, same `da-5895` epic. Still 1521 LOC `commands/dayview.rs` + 1166 LOC `commands/nowview.rs` + the layout-side duplicates.
- **§2 `dayview --watch` / `nowview --watch` duct-taped onto SQLite (PRAGMA data_version polling)** → **open**. Still polling. Filing as `vremya:` candidate to investigate when `datum-sync-agent` ships background writes.
- **§2 `db import` bulk path skips `try_insert` invariants** → **open**, partly mitigated by `_import` marker in `extra_data` (da-1el) but bulk path can drop the marker without test failure. Filing as `vremya:` candidate.
- **§2 Field-spec `render` polymorphism (`bool | { surface: bool }`) leaks** → status unchanged; `ResolvedRender` resolver still required at every consumer.
- **§2 EntryHistory has no compaction story** → **open**; Jasper's "file a bead" reminder still applies. Filing now as `vremya:`.
- **§3 Test clock injection via `-t` parser reuse (da-5ss)** → ✓ shipped, still load-bearing.
- **§4 bug 7: `set_time` thread-local breaks parallel worker tests** → unverified; flagging as `vremya:` audit candidate.

Archived. Jasper's biggest contribution: naming render-layer duplication as a single coherent problem (RenderPlan), which became epic `da-5895`.

— vremya, 2026-06-02
