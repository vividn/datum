# RETROSPECTIVE — daydumb from the write path

**Author:** polecat obsidian (rig: daydumb)
**Date:** 2026-05-06
**Workspace HEAD:** `polecat/opal/da-nnja@moto6qjm`, off `main` at `b7e74bb`
**Scope:** the write path. Insert / update / migrate-import / verb-derivation / conflict-strategy / persisted-intent. The plumbing under the verbs.

I shipped 45 commits, but the centre of mass is narrow: 21 in `crates/datum-commands/src/cmd.rs`, then `db/insert.rs`, `commands/update_core.rs`, `entry.rs`, `migrations.rs`, `schema.sql`. Quartz worked the surface; I worked the storage contract. The five-verb pivot (da-5ns sub-beads .2/.3/.4/.8), the `ConflictStrategy` API + tombstones (da-bfcm), the `extra_metadata` provenance split (da-tsw), the v2-aware migrate import (da-a8u, da-zio, da-7sr), and the e2e sync test suite (da-bra) — those are the load-bearing pieces.

This retro covers what I saw from there. It deliberately skips render, color, layout, dayview/nowview, and the parser mega-files — quartz, jasper, garnet and ruby covered those.

---

## 1. Where I lived

| File | My commits | Why I was there |
|---|---|---|
| `crates/datum-commands/src/cmd.rs` | 21 | log_entry, parse_value_token, extract_reserved_columns — every write goes through here |
| `crates/datum-core/src/db/insert.rs` | 5 | `try_insert` is *the* choke point. derive_verb_for_storage, raw_insert, conflict_queue gates |
| `crates/datum-core/src/entry.rs` | 5 | Verb enum, extra_metadata column, persisted-intent fields |
| `crates/datum-commands/src/commands/update_core.rs` | 5 | update/merge/rekey unification, auto-rekey on column mutations |
| `crates/datum-core/src/schema.sql` | 5 | extra_metadata, verb column, persisted-intent trio |
| `crates/datum-core/src/migrations.rs` | 4 | new column adds, the v2 import shape pivot |
| `crates/datum-parse/src/help/{commands,tests}.rs` | 8 | help.rs CommandSpec data model (da-8nm) |
| `crates/datum-cli/tests/sync_e2e.rs` | 1 (creation) | 1068-line multi-instance suite (da-bra) |

The seat is "what does the row look like by the time it hits SQLite, and how do we keep it honest across grammar changes."

---

## 2. What was harder than expected

### The five-verb pivot was an iceberg
da-5ns was supposed to be "store `verb` as a column." It became eight sub-beads spanning two months because the verb is *derived* from `(state, dur, following_state)` and the existing call sites set verb directly. Sub-bead .2 was `derive_verb` as a pure function (4-line body, 60-line docstring, 200-line table-driven test). Sub-bead .3 was wiring it into `try_insert`. Sub-bead .4 was the auto-rekey path: when an update mutates `occur_time` / `state` / `dur` / `following_state`, the slug must regenerate AND the verb must re-derive. Sub-bead .8 was the test sweep — six suites referenced fields that no longer existed (`start_time`/`end_time`, value-vs-state confusion).

The hard part wasn't the math. It was that *every write path had to funnel through one place* before I could change the derivation. `try_insert` already existed; the bulk-import path (`insert_entry_bulk_in_tx`) was a sibling. The discipline of "all writes call `derive_verb_for_storage`" only became enforceable after I confirmed `raw_insert` was the single entry to `try_insert` from both branches. That audit was a day. Worth it.

### The bulk import / live insert duality is real and unloved
`insert_entry_bulk_in_tx` exists because v2 backups have ten thousand rows and `try_insert` runs conflict_queue + history + slug-collision-with-rekey-retry on every one. The bulk path skips conflict_queue entirely. **It still goes through `try_insert` for the actual row write, so derive_verb runs uniformly** (good) — but conflict_queue dedup is a multi-row, cross-content-hash concern that is *only* enforced on the live path. A v2 backup with corrupt content_hash collisions imports without ever surfacing them. da-7sr fixed one symptom (premature derive_verb in the import wrapper); the deeper duality remains. Jasper noticed this from the import side ("a separate world from try_insert"). It's the same bug from two seats.

### The `IntentContext` thread-local
`cmd.rs::log_entry` reads `parser_version`, `raw_input`, and `alias_chain` off a thread-local set during the pre-pass — before clap. The comment says "must be captured *before* `inner` runs." This works today because every `datum X` invocation runs once and exits. da-lgf added REPL mode, which calls log_entry repeatedly in one process. **REPL works only because the pre-pass refreshes the thread-local on every iteration.** When async lands, or when REPL grows tab-completion that needs intent without committing it, this design will explode quietly. I built around it; I didn't fix it. It should be a parameter, not ambient state.

### `extra_metadata` had to be retrofitted
da-tsw split system provenance (the device that wrote the row, sync-source markers, internal-flag debris) out of `data` (user-typed JSON). The migration is mechanical; the contract is not. `data` has a written-by-the-user contract; `extra_metadata` has a written-by-the-system contract. There is no type that prevents a future patch from writing user data into `extra_metadata` or vice versa. The `--no-metadata` flag (da-wsax) and `datum edit` ($EDITOR round-trip, da-a4gc) both touch this seam: `datum edit` deserializes the entry, hands the user JSON, takes it back. **Today's edit path can rewrite `extra_metadata`.** It shouldn't be able to. It's a bug waiting to surface — I filed it as a note, not a fix.

### Persisted-intent infrastructure is half-built
`raw_input`, `alias_chain`, `parser_version` are all in the schema (76–78). They're written on every insert. `parser_version` is **never read by any consumer.** `alias_chain` is captured but no command re-resolves it. `raw_input NOT NULL DEFAULT ''` is the surrender quartz noted. The whole point — "when the grammar changes, re-parse history and report drift" — is unbuilt. The plumbing is in place, the tap is missing. See §5.

---

## 3. What was easier than expected

### `try_insert` as choke point paid off three times
After the verb pivot funneled through it, da-bfcm (`ConflictStrategy::{Ignore,Skip,LWW,Merge,Rekey}`) was a 2-day job, not a 2-week one. The strategy is an enum, dispatched at the top of `insert_entry_in_tx`, and every path below it reuses the same write call. da-tsw (extra_metadata) added a column without touching the insert API — a `serde_json::Value` slot, in and out. Ruby's update/merge/rekey wrappers (da-gd1j) then layered on `update_with_strategy` without touching insert at all. **One choke point, three orthogonal features, zero re-litigation.** This is the pattern I'd repeat.

### `dm check --verbs --fix` is a model safety net
Sub-bead 7 (`crates/datum-commands/src/commands/check.rs::collect_verb_drift`) walks every entry, re-runs `derive_verb`, diffs against the stored value. `--fix` rewrites drifted rows in place via `Db::set_verb` (no history row, since verb is derived). This caught the migrate-import bug in da-7sr immediately — the v2 import was running derive_verb prematurely with a `None` state, classifying everything as `Verb::Point`. `dm check --verbs` flagged 2400 rows on a synthetic backup; the fix was one line; the test was already there. **Build the drift detector before you build the migration.**

### Sub-bead chain discipline (da-5ns .1–.8) actually worked
Eight sub-beads, dispatched serially, each with its own review and merge. No long-lived branch. Each sub-bead was 1–3 days. The schema migration landed in .1, the pure function in .2, the wiring in .3, the auto-rekey in .4, the inverse-state migration in .5, the legacy-verb deprecation announce in .6, the `dm check` tooling in .7, the test sweep in .8. **Splitting an invasive refactor into "one PR can be reverted without losing the others" is the only sane way to ship them in this swarm.**

### The migrate-import fixture roundtrip (da-zio) caught everything
A synthetic CouchDB-shaped JSON fixture in `crates/datum-cli/tests/fixtures/`, imported with `datum migrate import`, then asserted against. After da-zio shipped, every subsequent migrate-import bug — da-a8u v2-shape handling, da-7sr the premature derive_verb — was a one-line fix because the regression test was already there.

### The TOML kit format slice (da-d4t.1) just worked
`#[derive(Serialize, Deserialize)]` on `KitManifest`/`FieldSpec`, `toml::from_str`, done. The `[[fields]]` table-array shape is what users would write by hand if they didn't know Rust. No fight with serde. The signed-bundle slice (.2–.7, others' work) layered on cleanly.

---

## 4. Bugs I noticed but wasn't asked to fix

1. **`datum edit` can rewrite `extra_metadata`.** $EDITOR round-trips the whole entry as JSON. The deserializer doesn't reject `extra_metadata` mutations from the user-text payload. If a user edits the JSON they're shown, they can scribble into system-only territory. Fix: route edit through a path that splits user data from extra_metadata at parse time, like `--no-metadata` does on the input side. (`crates/datum-commands/src/commands/edit.rs`.)

2. **`alias_chain` is captured but never replayed.** When a user runs `datum vbp correr` and that expands via alias to `datum note word srcLang=pt pos=verb correr`, we store the chain. No command reads it. If aliases change, history shows the post-expansion result with a frozen chain that no longer matches today's resolution. (`crates/datum-core/src/entry.rs:257`.)

3. **`parser_version` is Schrödinger's column.** Defaults to 1, never read. Same shape as `alias_chain`: written, never consulted. (Quartz noted this in their §5.5; I'm seconding it from the write side.) (`schema.sql:78`, `entry.rs:259`.)

4. **Tombstone vs. cross-device update is untested.** `da-bfcm` added soft-delete tombstones. Sync (da-bra) tests two-device convergence. Neither tests "device A deletes, device B updates the same slug, sync." The tombstone wins by `LWW` if it's newer, but the merge codepath in `update_core.rs` has a `Tombstoned` error variant (`error.rs:34`) that aborts. If the order arrives reversed, behavior depends on which arrives first. (`crates/datum-cli/tests/sync_e2e.rs` is the right place.)

5. **Bulk import skips `conflict_queue`.** `insert_entry_bulk_in_tx` is dedup-by-content-hash only. A backup with a content_hash collision against an *existing* live row gets silently skipped. The conflict_queue exists to surface these for human triage; the bulk path bypasses it. Either route bulk through conflict_queue (slower) or write a `dm check --bulk-imports` that scans recent imports for what would have been queued.

6. **`derive_verb_for_storage` special-cases legacy verbs in code, not types.** Sub-bead 8's promised sweep of `Verb::{Occur,Start,Stop,Add}` didn't happen. Today (`db/insert.rs:41`) the function pattern-matches on those exact variants to canonicalize state. Until they're removed, the canonicalization rule is in code in two places — here and `Verb::FromStr`. Inconsistent input could land an entry whose verb the storage layer disagrees with. `dm check --verbs` would catch it; nothing prevents it.

7. **History rows aren't compaction-aware.** `entry_history` (`schema.sql:271`) records every mutation. `raw_input NOT NULL` on the history table means a user-asserted "this is the truth at write time" survives, which is right. But there's no policy for compaction. A row that's been edited 50 times has 50 history rows forever. Pre-v1 acceptable. Post-v1 it's a unbounded growth path on long-lived users. Jasper noted this from the layout side; I'm noting it from the write side.

---

## 5. The non-obvious improvement: `dm reparse`

> *Build the tool that consumes `raw_input` + `parser_version`. It is the entire reason those columns exist and it is the killer feature behind daydumb's "your text survives grammar changes" promise.*

The persisted-intent trio is plumbing for one feature: **when the parser changes, walk the entry log and report (or apply) drift.** Today the plumbing is in place. The tool is missing.

```
$ dm reparse                     # report only
3 entries: parser_version=1, current=2
  da-vgma_2024-03-16T... :  --dur 1:30  →  --dur PT1H30M
  ...
$ dm reparse --fix               # rewrite stored shapes from current parse of raw_input
$ dm reparse --since da-5ns      # cohort by version range
$ dm reparse --diff <slug>       # show before/after for one row
```

What it does: for every entry, take `raw_input` and `alias_chain`, re-run the current parser against them, build a fresh `Entry`, diff against the stored row, surface the deltas. `--fix` rewrites stored columns to match the re-parse (with `dm check --verbs --fix` rerun afterward to catch verb drift induced by `state`/`dur` mutations).

**Why this is the move:**

1. **It validates the invariant we already advertise.** The README and CLAUDE.md say grammar changes don't lose user history. Today that's a promise without a test.
2. **It bumps `parser_version` from a stored constant to a useful column.** Today its value is always 1. With reparse it becomes a cohort marker: "all entries at v1, here's the v2 delta, decide per-cohort."
3. **It catches alias drift.** `alias_chain` was stored to enable exactly this — "this entry came in via the `vbp` alias which today expands differently; here's what would land if we re-evaluated."
4. **It pays for itself in the migrate-import path.** da-a8u handled v2-shape import; v3 is a `dm reparse` away. A polecat who lands a parser change can run `dm reparse --since <version>` on a synthetic corpus and see the diff before merging.
5. **The tool is small.** Maybe 400 LOC: walk entries, call existing `Cli::from_argv` against `raw_input`, run existing `log_entry` payload-build path with a "no-write, return Entry" mode, compare. The hard part is *reusing the parser without re-executing it as a write*. That refactor pays off elsewhere too: `datum --dry-run` becomes free.

**Why quartz wouldn't propose this.** Quartz works the surface — color resolution, comma tokenization, the `dm check` ergonomics. They wouldn't notice that `parser_version` is unread because they never write to it. From the write seat, `parser_version` is a column I touch on every single insert and have never seen any code read. That asymmetry is the tell.

This is what I'd ship if given a week.

---

## 6. My area's relationship to the whole

The write path is a **coupling hub by semantics, a leaf by code volume.**

- `db/insert.rs` is ~1000 LOC. `commands/update_core.rs` is ~500. `verb.rs` is small. `entry.rs::Verb` plus `derive_verb` is the entire derivation rule.
- But: every other crate eventually writes through here. The CLI's log_entry. The migrate path. The sync client (`datum-sync-client/src/ops.rs:151` writes raw_input/alias_chain/parser_version on incoming sync). The web app's WASM build will write here too once sync lands.

What that means for refactors:
- **Adding a column** (extra_metadata, parser_version, occur_time_precision) is mechanical from this seat. INSERT and UPDATE stmts in insert.rs, row_mapper.rs deserialize, schema.sql + a migration. Eight files. Done in a day.
- **Changing the write contract** (e.g. removing legacy `Verb` variants) is a swarm-wide refactor. Every command builds an Entry. The legacy variants live in `Display` / `FromStr` / `derive_verb_for_storage` special cases. Sweeping them is sub-bead 8 of da-5ns and it didn't happen because the blast radius went beyond what one polecat session can hold.
- **Breaking the choke point** (allowing an end-run around `try_insert`) destroys the verb-derivation invariant. The bulk path got special permission for performance and immediately introduced a duality (§2). Any future "fast write path" should be reviewed against "what `dm check --verbs` will say on Tuesday."

If I refactored my area I would:
- Make `IntentContext` a parameter, not a thread-local. Eight call sites. Removes the future REPL/async pitfall.
- Newtype `extra_metadata` so `datum edit` cannot accidentally write to it.
- Build `dm reparse` (§5).
- Sweep `Verb::{Occur,Start,Stop,Add}` and the matching state-promotion special cases. Single file. Closes sub-bead 8 honestly.

Three of those four are local. The Verb sweep crosses crates. That ratio (3:1 local:cross-cutting) is what makes this seat tractable — the code is small, the contract is large.

---

## Closing — obsidian's view from the storage layer

The write path is in good shape. `try_insert` as choke point paid off three times. `derive_verb` as a pure table-driven function is the cleanest invasive refactor in the codebase. `ConflictStrategy` carved naturally into the strategy slot at the top of `insert_entry_in_tx`. `dm check --verbs --fix` makes the whole thing self-auditing. The pieces I built fit.

What I didn't finish: the persisted-intent feature behind the columns I add to every row. That's the gap. `parser_version` is currently a constant and a regret. `dm reparse` closes it. If one move from §5 lands before v1, make it that one.

The legacy `Verb` variants are an embarrassment to me personally — sub-bead 8 was *my* commit (`4a847c8 chore(da-5ns.8): test sweep`) and I shipped the test sweep without the type sweep. Quartz called it out from the surface; I'll call it out from the seat that owns the type. Cut them at v1.

— obsidian, 2026-05-06

---

## Status (as of 2026-06-02): Closed

- **§5 `dm reparse` — consume `raw_input` + `alias_chain` + `parser_version`** → ✓ shipped (`da-bkak` + `da-g5uk`). The persisted-intent trio now has its consumer. Obsidian's "if one move from §5 lands before v1, make it that one" — landed.
- **Legacy `Verb::{Occur,Start,Stop,Add}` sweep — Obsidian's "embarrassment to me personally"** → ✓ shipped (`da-8y2q.2`). Sub-bead 8 finally swept the types alongside the tests. `xtask check no-legacy-verbs` enforces.
- **§2 five-verb pivot iceberg / `try_insert` as choke point** → still load-bearing; `ConflictStrategy` + `extra_metadata` + LWW sync_merge all layered through `try_insert` without re-litigating.
- **§2 bulk-import / live-insert duality** → still present; `insert_entry_bulk_in_tx` still bypasses conflict_queue. The `_import` marker compensates locally; sync convergence assumes it. Filing as `vremya:` audit candidate (jasper's §4#2 names the same thing).
- **§2 IntentContext thread-local** → unchanged; will bite under REPL/async per Obsidian's own warning. Filing as `vremya:` candidate to thread it as a parameter.
- **§2 `extra_metadata` user-vs-system contract has no type enforcement** → status unchanged. `datum edit` can still scribble into system territory. Filing as `vremya:` candidate to wrap with a newtype.
- **§2 persisted-intent half-built** → ✓ resolved by `dm reparse` shipping.
- **§3 `try_insert` choke-point pattern + `dm check --verbs` drift detector** → still load-bearing; the model.

Obsidian's *"build the drift detector before you build the migration"* is now a codified pattern. Archived.

— vremya, 2026-06-02
