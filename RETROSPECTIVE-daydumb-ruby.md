# RETROSPECTIVE — daydumb, view of polecat ruby

**Author:** polecat ruby (rig: daydumb)
**Date:** 2026-05-06
**Workspace HEAD:** `polecat/garnet/da-l0vk@motnv872`, off `main` at `374740f`
**Commits to main as ruby:** 4 (`git log --author=ruby origin/main`):

- `c4050cc` da-gd1j — `update`, `merge`, `rekey` commands on top of `ConflictStrategy`
- `4c5cf5a` da-t9xc — split `views.rs` into a module dir; ported `timing` / `state_change` / `duration_sum` / `datum_sub_human_id`
- `07aed88` da-ng2y — created `crates/datum-layout` and the `datum dayview` SVG path
- `cf8b39a` da-u4k.5 — `-T/--no-timestamp` routes log verbs through the timeless `Verb::Add` path

So I touched four areas: command-layer mutators (update/merge/rekey), the `views/` module dir under `datum-core`, the entire `datum-layout` crate (created it), and one ribbon of `datum-commands::cmd::log_entry` (the `-T` gate). Quartz wrote the general retro from the perspective of someone making one-byte edits across the whole tree. I'm writing from the perspective of someone who handed three modules over to other polecats and got to watch what they did with them.

---

## 1. What I actually built and where it lives now

### `crates/datum-layout/` — created in da-ng2y

I shipped this crate with one renderer (`svg::render_svg`), one layout function (`layout_day`), and a `--json` flag on `datum dayview` to dump the `Layout` POD as a bridge for the PWA until WASM bindings landed.

Today the crate has **six source files, 2997 LOC**:
```
dayview.rs            1013   (other polecats: da-vp7 multi-day, da-a7f vertical, da-tbe orientation, da-glb gridlines)
dayview_svg.rs         670   (SVG renderer split out)
nowview.rs             212   (da-vw4)
nowview_svg.rs         484   (da-vw4)
nowview_terminal.rs    554   (da-jyi — ANSI in a wasm-clean crate, more on this below)
svg.rs                 149   (my original "any-Layout-to-SVG" — now orphaned)
lib.rs                 319   (POD types, re-exports)
```

My `LayoutEntry` / `LayoutConfig` / `Layout` POD trio survived (lib.rs:24-90). The original `svg::render_svg` still lives at `svg.rs:1` but **nothing in `datum-commands` calls it any more** — `commands/dayview.rs` and `commands/nowview.rs` go through `dayview_svg::render` and `nowview_svg::render`. Quartz's retro flagged the dayview/nowview duplication at the command layer; the layout layer has the same shape. I'd cut `svg.rs` outright at v1.

### `crates/datum-core/src/views/` — created the directory in da-t9xc

I split `views.rs` into a dir and added four views on top of a CouchDB-shaped `View::map` + `View::reduce` + `supports_reduce` trait. The entire trait was thrown away in da-t6t.3 (R3) — `View::query(db, params) → Vec<ViewRow>` is what replaced it. The four view *names* and their key/value shapes survived. The implementations were rewritten SQL-native with field-pushdown filters and a new `entries_live_field_start` partial index (migrations.rs v5→v6).

Net: my framework died, my view shapes lived. That's the right outcome — I'll explain why in §3.

### Update / Merge / Rekey — da-gd1j

I added these on top of A4's `ConflictStrategy`. They are now thin wrappers in `commands/{update,merge,rekey}.rs` that all call into `update_core::update_with_strategy` (`commands/update_core.rs:79`). The `Tombstoned` error variant (`error.rs:34`) and `get_entry_including_deleted` helper (`db/lookup.rs:143`) are still there, used unchanged. Ruby's "reject tombstoned rows distinctly" contract carried through every refactor since.

### `-T/--no-timestamp` — da-u4k.5

Lives in `cmd::log_entry` (`crates/datum-commands/src/cmd.rs:484-491` and `:1393-1410`). It's two ribbons: a verb override (`verb = Verb::Add` if omit_timestamp), and `check_no_time_flags` rejecting `-t`, `-d`, `--dur`, `-y`, `-q`, `-z`. Both ribbons are exactly where I left them, with one S6 follow-on (da-u4k.6 `-L/--last-state`) that respected my time/no-time bifurcation rather than fighting it.

---

## 2. What was easier than expected

### `ConflictStrategy` was a gift

When I started da-gd1j I expected to write the merge / overlay / rekey codepaths from scratch. A4 had already extracted `ConflictStrategy` (`Skip` / `LWW` / `Merge` / `Rekey`) and routed `try_insert` through it. `update` became "fetch row, build patched payload, re-insert with `Merge`"; `rekey` became "fetch row, mint new ULID + slug, insert with `Rekey`". The hard problem (what does "merge two entries with the same content_hash" mean) was solved one bead earlier. This is the rare case where a generalization landed *before* the consumers and was the right shape.

### POD-shaped layout types serialize for free

`#[derive(Serialize, Deserialize)]` on `LayoutEntry` / `LayoutConfig` / `Layout` got `--json` working in five lines. The Svelte side can `JSON.parse` that and bind directly. No WASM yet; not needed yet. **The `--json` flag is daydumb's actual web/native bridge today, three weeks after I shipped it as a stopgap.** That's a healthy outcome and probably means the WASM bindings should be deferred indefinitely.

### Splitting `views.rs` into a directory was free with `mod.rs`

Rust's module system made the split mechanical: rename `views.rs` to `views/mod.rs`, create per-view files, no public-API change. This is the part of the language I miss most when I drop into a Python codebase.

---

## 3. What was harder than expected

### Porting from CouchDB-emulation was wasted work

The four views I ported (da-t9xc) leaned on a `View::map(db) → Vec<ViewRow>` + `View::reduce(rows) → ViewRow` + `supports_reduce(): bool` trait that mirrored CouchDB. I wrote `state_change` as a per-field walk in Rust because that's how datum did it. I wrote `sub_human_id` as a fanout-every-prefix-as-its-own-row because that's what CouchDB needed for shortest-unique-prefix lookups.

R3 (da-t6t.3) replaced all of it with SQL. State_change is now a window-function query. Sub_human_id pushes down a `LIKE` filter and only fans out prefixes for matching entries. The R3 author was right and I was wrong: **I ported the framework, when I should have ported only the views and let SQL do its job.** The map/reduce ceremony was load-bearing for nothing — `datum-core` was always going to be SQLite-backed; emulating CouchDB's evaluation model on top of SQLite was building a chair on top of another chair.

This is a generalizable lesson about "spiritual successor of X" rewrites: porting the **inputs** and **outputs** of a system gives you compatibility; porting its **internal evaluation model** gives you a museum.

### `nowview_terminal.rs` should never have lived in `datum-layout`

When da-jyi added `--terminal` for `datum nowview`, the renderer (ANSI escape codes, terminal width detection) got dropped into `crates/datum-layout/src/nowview_terminal.rs`. The crate is supposed to be wasm-clean — it's in our `Cargo.toml` build matrix as `cargo build -p datum-layout --target wasm32-unknown-unknown`. Today that target doesn't actually fail because the terminal renderer happens to use only `std::fmt`, but it's an accident. The right home is `datum-commands::render::nowview_terminal`. (Quartz noticed this from outside; I'm seconding it from inside the crate — `datum-layout` is not a renderer crate, it's a *layout* crate. I named it that on purpose.)

### Sub-bead chains drift faster than parent docstrings

S5 (`-T`) shipped with a comment saying "S6 will introduce `-L/--last-state` and must respect omit_timestamp." S6 shipped a week later and did respect it (`cmd.rs:593`), which is great — but **only because the implementer happened to read S5's commit message**. There's no test that fails if S6 silently drops the gate. The sub-bead format buys the right author handoff but leans on prose discipline for invariant preservation. A dedicated "no-timestamp + last-state is rejected" test in `tests/no_timestamp.rs` (159 LOC, all mine) would have been right; I didn't write it because S6 didn't exist yet when I shipped S5.

### Test gating wiped out my own coverage

I shipped `crates/datum-core/src/views/{timing,state_change,duration_sum,sub_human_id}.rs` with local `#[cfg(test)]` modules that exercised every view's `map` and `reduce` paths. They're all `#[cfg(any())]` now (see line counts: state_change.rs:294, timing.rs:132, duration_sum.rs:114, sub_human_id.rs:143). The R3 rewrite gated them off because `View::map`/`View::reduce` no longer exist; new SQL-shaped tests were never written. **Four of the eleven `#[cfg(any())]` test modules quartz flagged are mine.** I'd rewrite them as part of the same PR that deletes them — for each view, prove: (a) one row per source entry, (b) tombstones excluded, (c) `--key` filter pushed down, (d) field arrays from da-u4k.12 are handled (state_change in particular — see §4).

---

## 4. Bugs I suspect or noticed but wasn't asked to fix

1. **`crates/datum-layout/src/svg.rs` is dead code.** 149 LOC of generic `render_svg(&Layout)` from da-ng2y. No call site exercises it. `commands/dayview.rs` calls `dayview_svg::render`; `commands/nowview.rs` calls `nowview_svg::render`. Delete or repurpose — today it's a tax on `cargo doc` and a tar pit for the next polecat to grep "render_svg" and find the wrong one.

2. **`state_change` SQL post-R3 may not handle array-states from da-u4k.12.** `switch f a,b` produces `state = ["a","b"]` (JSON array). R3's state_change emits transitions assuming scalar state — see `views/state_change.rs:53-130` for the diffing logic. Two adjacent switches with overlapping array elements should probably emit individual transitions per element, but the current code compares whole JSON values. Worth a test case before v1.

3. **`update` command's "shallow overlay" semantics are surprising.** `dm update <id> foo=bar` replaces top-level `foo` in `extra_data`; `dm merge <id> foo=bar` deep-merges. The verb names don't telegraph the depth difference — a user typing `dm update mood.score=8` expects the dotted path to traverse, but it lands as a literal top-level key `"mood.score"`. Either document this on `--help` more loudly or accept dotted-path syntax in `update`.

4. **`rekey` collision behavior is opaque.** If `--id <template>` interpolates to a slug that already exists, my original code relied on the `Rekey` strategy at the DB layer to mint a new ULID. After the slug-as-PK pivot (da-ndd.10) the ULID is gone — `hid` is now a base36 random with a one-shot retry. I didn't update `rekey`'s error story for that. The collision-during-rekey path needs an explicit test in `commands/tests/rekey_*.rs`.

5. **`nowview_terminal.rs` width math assumes monospace cells.** Wide-character / emoji handling in field labels will misalign columns. There's no `unicode-width` dep in `datum-layout/Cargo.toml`. A user with a `field=📚` will wreck the alignment. Filed as a future bead candidate — not urgent but real.

6. **`sub_human_id` view leaks CouchDB-isms in its API.** It still emits one row per prefix when called without `--key`, even though the SQL-native path could just return shortest-unique-prefix directly. The trait shape forces it to mimic the old fanout because CLI `datum map datum_sub_human_id` expects every prefix as a row. Change the contract: `View::lookup_shortest(prefix) → Option<Entry>` is what the C3 callers actually want; the row fanout is an implementation detail of CouchDB-shaped views that no longer fits.

7. **`-T/--no-timestamp` doesn't compose with batch import.** `datum db import --from <couchdb.json>` could legitimately produce timeless entries (the source backup may contain `note`-shaped docs without `occurTime`). `migrate import` paths bypass `log_entry` entirely, so the `-T` flag's "verb becomes Add" rule doesn't fire there. Verb derivation on the import path needs to know about timeless rows. I haven't checked whether the import path does the right thing for old `note`-typed docs missing `occurTime`; suspect it errors instead of producing `Verb::Add`.

---

## 5. One non-obvious improvement (the only section that matters)

**`datum-layout` and `datum-core::views` are the same idea — make them one.**

Today the codebase has two parallel "named projection of the entry log" systems:

- `datum-core::views::{TimingView, StateChangeView, DurationSumView, SubHumanIdView}` — each a SQL-backed query producing `ViewRow { key, value, id }` POD. Registered by name in `views::builtins()`. Called via `Registry::query(name, db, params)`.

- `datum-layout::{layout_day, ...}` (and the in-progress `nowview_*`) — each a function from entries + config to a typed `Layout` POD. Today they're free functions. The intent is "one named, parameterizable spatial projection of entries," which is exactly what a view is.

Both are: (a) named, (b) parameterizable, (c) read entries from `Db`, (d) produce a typed POD ready for downstream consumption. The only differences are (1) the output shape (`ViewRow` vs. `Layout`), and (2) one was extracted as a crate, the other lives in `datum-core`.

If they were unified — a `View` trait with two associated outputs (`type Row` for tabular, `type Spatial` for layouts — or a single output enum) — you'd get:

1. **One registry, one CLI surface.** `datum view <name>` already exists for tabular views; `datum dayview` and `datum nowview` would become `datum view dayview` (with `--render svg|terminal|json|png`). The `--json` flag I shipped becomes the default behavior for any view; renderers are *consumers* of the registry, not separate commands.

2. **The PWA gets every view for free.** Today the PWA can call WASM-compiled views over OPFS data; tomorrow it can do the same for layouts. Right now the layout crate is a separate WASM target story.

3. **`datum-layout` becomes a renderer, not a layout-er.** The layout *math* (time→x, field→y, axis ticks) moves into `datum-core::views::dayview`, which produces a `Layout` POD as its `Row` type. `datum-layout` becomes `datum-render` — a crate of consumers (`render_svg(layout)`, `render_terminal(layout)`, `render_png(layout)`). Quartz's "render vs. layout boundary is blurry" complaint dissolves automatically: `render_*` lives in the renderer crate, `view_*` lives in the view crate.

4. **`nowview_terminal.rs` finds its home.** ANSI in `datum-render` is fine; ANSI in `datum-layout` is a layering violation.

5. **Kit bundles ship spatial views the same way they ship tabular ones.** Today `kit.rs` knows about SQL views and field specs; spatial views are out of band. Unifying lets a kit ship "dayview-mood" as a parameter preset on top of the dayview view. Plug-in story extends without new abstractions.

The work is not small but it's bounded: extract the layout math into `datum-core::views::{dayview,nowview}` as `View` impls; rename `datum-layout` to `datum-render`; collapse `commands/dayview.rs` and `commands/nowview.rs` into the existing `commands/view.rs` dispatch. Estimated ~1500 LOC moved, ~300 LOC deleted (the duplicated context-building quartz mentioned).

This is the move quartz couldn't propose — they didn't write the layout crate, so they couldn't see that it's a view registry trying to be born. **Do this before v1, not after**: post-v1 the `datum dayview` command surface is locked.

---

## 6. My area's relationship to the whole

`datum-layout` is **a leaf with hub ambitions, frozen mid-pivot**. It:
- has zero deps on `datum-core` (good, intentional — wasm-clean)
- is consumed only by `datum-commands::commands::{dayview,nowview}` (one upstream)
- wants to be consumed by the PWA (`--json` is the lifeline; WASM bindings are stalled)
- got a renderer (`nowview_terminal.rs`) it shouldn't own

The pivot I'm proposing in §5 commits it to "renderer crate" — that's the honest shape.

`datum-core::views` is **a clean leaf, almost too thin post-R3**. The trait is `fn query(&self, db, params) → Vec<ViewRow>`. The registry is a `BTreeMap<String, Box<dyn View>>`. Could collapse to a `match name { "timing" => …, … }` table of free functions. The trait's only job is dynamic dispatch by name; there's no generic algorithm parameterized by it. Either grow it to justify the trait (the §5 unification) or shrink it to functions.

`update`/`merge`/`rekey` is **a hub for write-time conflict resolution**, sitting on top of `update_core::update_with_strategy`. It's the cleanest of my four areas — `commands/{update,merge,rekey}.rs` are 12-line shims, the work happens in `update_core.rs`. This is exactly what quartz wished for the rest of `commands/` (66 R2-stub bodies that all `bail!`); my three commands are the "what good looks like" reference. Worth pointing the next R2-carve-up polecat at this trio as the shape to copy.

`-T/--no-timestamp` is **a single ribbon through the mega-file**. Two `if args.time.omit_timestamp { … }` checks in `cmd::log_entry` and one helper. Touching it doesn't touch much else. But this is the case where the mega-file's badness is *least* felt: small cross-cutting flags are exactly what a 5500-LOC dispatcher does well. The pain of `cmd.rs` hits cross-verb flag *combinations*, not single-flag adds.

---

## 7. What I'd do differently next time

- **Stop porting. Reimplement.** I lost two days of effort writing `View::map`/`View::reduce` ceremony for views that were always going to live in SQLite. If the storage layer is different from the source's, the evaluation model should be too. Port the I/O contract; reimplement the body.

- **Make the `--json` debug bridge a first-class output for every view.** `--json` on `dayview` was the most useful thing I shipped, by accident. Every command that produces a typed POD should expose a `--json` lifeline by default. This makes the PWA story incremental rather than WASM-or-bust.

- **Write the test before the gate.** The `check_no_time_flags` rejection-path test (the `mixes_*_rejected` cases in `tests/no_timestamp.rs`) covers what I wrote. The interaction with future flags is not covered. When I add a gate, I should add a "any time-shape flag rejects timeless" property test that picks up new flags automatically as they're added to `LogTimeArgs`.

- **Don't put a renderer in a layout crate.** I created `datum-layout`. The next polecat put a terminal renderer in it. That's on me for not establishing in the README that this crate emits POD only — renderers belong elsewhere. The crate-level docstring (`lib.rs:3`) says "pure layout for daydumb's dayview"; it should say "pure layout, no I/O, no rendering" with a `#![forbid(unsafe_code)]`-style invariant and a CI check.

---

## Closing — ruby's view

My areas are a barometer for two project-wide patterns: (1) the codebase rewards **small typed PODs at crate boundaries** (`Layout`, `ViewRow`, `Entry`) — every place those exist, the crate splits cleanly and the test surface is real; and (2) the codebase is forgiving about **frameworks that turn out to be wrong** — my CouchDB-shaped view trait was thrown away in three weeks with no public-API damage because the inputs/outputs (registry by name, `ViewRow` POD, `--key`/`--group` params) were the contract and the trait was implementation detail. Both patterns argue for the §5 unification: same POD discipline, same forgiving-trait pattern, applied to spatial views.

The single move I'd make before v1 is the view-registry unification (§5). If autonomy for one week and no backlog, that's the work. It deletes ~300 lines, moves ~1500, eliminates two of quartz's "leaky boundary" complaints, and gives the PWA a stable interface that doesn't depend on WASM landing.

— ruby, 2026-05-06
