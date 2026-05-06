# RETROSPECTIVE — daydumb from the schema/alias/config seat

**Author:** polecat jade (rig: daydumb)
**Date:** 2026-05-06
**Workspace HEAD:** `polecat/obsidian/da-ncfq@moto5imq`, off `main` at `b7e74bb`
**Scope of my contributions:** 15 commits — the R1 crate split (`datum-commands`), the slug-as-PK + hid schema pivot (da-ndd.10), the auto-alias / D3 system (`derived_aliases`, da-gnj.3), the layered user config (`datumrc.yml`, da-mdjq), and the tail surface enhancements (`--watch`, `-L/--last-state`, `--dry-run + --json`, the da-u4k.1/.6/.8 trio). Quartz's general retro stands; this one is what only I can say.

---

## 1. Where I lived

`git log --author=jade origin/main --name-only` puts the heat in five places:

- **`crates/datum-commands/src/cmd.rs`** (created in da-t6t.1 step 1; touched in 5 of my 15 commits). I am the polecat who *moved* this file. Quartz calls it the most painful file in the workspace. They are right and I built the move that left it that shape.
- **`crates/datum-core/src/db.rs` / `schema.sql` / `migrations.rs`** (7 of 15). The slug-as-PK pivot and the alias-table column add (`derived_from_field`, da-ndd.6) ran through this trio.
- **`crates/datum-core/src/field_spec.rs`** + the `derived_aliases` machinery — the centre of D3.
- **`crates/datum-commands/src/config.rs`** — layered yaml + env + flag, with `Source` tracking for `datum config show`.
- **`crates/datum-cli/tests/tail_cmd.rs`** + `commands/tail.rs` — the tail surface (`--watch`, `-L`, `--dry-run`, `--json`).

Three of those are coupling hubs (cmd.rs, schema.sql, field_spec.rs). One is a leaf (config.rs). One is a feature (tail). The mix is what I want to talk about.

---

## 2. What was harder than expected

### The R1 split looked like a directory move. It was a graph rewrite.

da-t6t.1 step 1 ("create datum-commands crate, move output/check/sync") is one commit. It is also the only commit in this repo's history where the diff is "1 file changed, 0 insertions(+), 0 deletions(-)" lying through its teeth — `git mv` shows the same logical change as a 4000-line shuffle. The hard part wasn't the move. It was:

1. Every `crate::` path in cmd.rs had to be rewritten to either `datum_commands::` or `datum_cli::`, depending on which side of the new boundary the symbol lived. About 200 such references.
2. `datum-cli/src/lib.rs` became 100% re-exports (the shim quartz mocked) because every integration test imports `datum_cli::cmd::foo` and we didn't want to touch 44 test files in the same diff.
3. `output` and `check` had to be the first commands moved because `sync` depended on them — nothing else compiled until those three were across the boundary.
4. `xtask` had to learn the new crate exists.

Step 2 (split cmd.rs into per-command files, da-t6t.1 step 2) made the directory list look richer but moved no behaviour. Quartz's "66 R2-stub functions that all bail" is the count of `<verb>_from_argv` and `<verb>_from_string` shims I created as placeholders for R2. **R2 was supposed to be the next slice the next week. It hasn't been started. That's on the work pipeline, not on the design — but the visible debt is mine.**

### Slug-as-PK (ndd.10) burned three days for one good reason

The original schema used a ULID `id` column as PK. da-ndd.10 swapped to `hid TEXT PRIMARY KEY` (random 12-char base36) + a `slug` UNIQUE index. The conceptual change is clean. The implementation hit four sharp edges:

1. **Migration `m_19_to_20`** had to walk every existing entry, hash its old ULID into a deterministic hid (we want round-trip for migration tests), and rewrite every FK (entry_history, sync_conflicts, recent_hids). Twelve points of edit, three of which had typo'd column names that compiled because SQLite is permissive about prepared statements with bound names that don't match.
2. **`db_import` fast-path** (the bulk migrate-from-CouchDB path Jasper called out) bypasses `try_insert` and writes raw rows. After ndd.10 the raw rows had to fabricate hids without colliding. The retry-once-on-collision logic (da-p5j) lives in `db/insert.rs` but the bulk path got its own copy. Two collision policies, one user concept.
3. **`get_cmd` parity tests** (`crates/datum-cli/tests/get_cmd.rs`) were checking entries by short ULID prefix. Twenty-odd assertions had to switch to hid prefix. The diff in `5261037` is the boring follow-up commit that exists because the same fix shows up at every readback site.
4. **The down-migration** (`down_20_to_19`) is destructive: hids cannot be reversed to ULIDs. Migration roundtrip tests for v20→v19→v20 had to mark v19 as a fresh-state baseline, not a value-preserving point. The framework supports this via `NotReversible`, but I had to discover it by trying the cheap path first.

The bug shape I fear most: **a future migration that touches `entries` and forgets one of the FK tables.** `entry_history.entry_hid` is the only index; nothing in `db/mod.rs::migrate()` enforces "if you rename `entries.hid`, propagate." Schema-level FK is off (SQLite default). I considered enabling `PRAGMA foreign_keys=ON` but the older migrations don't satisfy it.

### Auto-alias terminal-verb pre-pass ordering

da-gnj.3 step 3 (`0ce7272`) — the user types `datum mood switch happy`. The pre-pass needs to:

1. Recognize `mood` is a registered field, swap to `datum switch mood happy`, AND
2. Not double-expand if `mood` is also a registered alias for `switch mood`, AND
3. Capture the user's literal `raw_input` BEFORE any of (1) and (2) happens, AND
4. Capture an `alias_chain` of every expansion.

The current `expand_argv_with_db` does this in a specific order (terminal-verb override → alias expansion → field-position swap), and the order matters — flipping the first two breaks `datum vbp correr "to run"` (the alias pattern in CLAUDE.md). I figured this out by writing acceptance tests in `tests/alias_acceptance.rs` and watching combinations fail. **Every order-of-operations decision is encoded only in test cases, not in the type system.** A future polecat who swaps two passes will not be told.

### `derived_from_field` column was a quiet schema rip

da-ndd.6 (`9ca35b0`) was sold as "fix: migrate pre-a0 aliases table to add derived_from_field". One ALTER, one migration. What it actually changed: the aliases table grew a new conceptual axis — *user-defined* vs *derived-from-spec*. The column is the only thing distinguishing them. Every code path that reads `aliases` now has to remember to filter on this column or accept "system aliases will appear in `datum alias list`". `datum alias list` got it right. Other queries didn't, and I'm not sure I caught them all.

---

## 3. What was easier than expected

### Layered config (mdjq) was almost suspiciously clean

`datum-commands/src/config.rs` Layered config: `SourcedValue<T> { value: T, source: Source }` + a `RawConfig` that overlays yaml > env > default. Per-key source tracking is just five enum variants. `Config::load` is one function that walks the layers. Total ~600 lines, did one job, zero follow-up bug beads. `datum config show` printing `editor = vim (yaml)` worked first try.

The reason it worked: I had no choice about what the config *means* (env var names were already canonical via `DATUM_*`). I only chose how to layer them. Constraint = clarity.

### Verb::Note → Verb::Add was painless

da-gnj.3 step 1 (`0dbb66f`) renamed the canonical `Note` verb to `Add` and demoted `note` to a `FromStr`-only compat string. I expected a migration nightmare; I got a `Display`/`FromStr` swap, a `Verb::Add` constant rename, and four call-site touches. The reason: **the verb enum is small and parse-time fan-in is the only entry point.** Anything that types `note` becomes `Verb::Add` at the door; anything that asks for `Verb::Add.to_string()` gets `"add"`. There is no third place where verbs leak as strings.

This is the model the legacy `Occur/Start/Stop` variants quartz wants swept *should* follow. It wasn't hard to do for `Note`. It hasn't happened for the others because there's no champion, not because it's blocked.

### `tail --watch` is 60 LOC of polling

I expected the watch flag to mean a daemon, file-system events (notify-rs?), or at least a SQLite hook. It's a `loop { sleep; query; if rows changed, redraw }`. The watermark is row count — cheap, correct under appends, broken only if a sync rewrites history during the watch window (rare; obvious when it happens). The first version shipped before I'd thought of an alternative.

### `--dry-run + --json` (u4k.8) restored a feature for free

The earlier rewrite of `db import` had dropped the `--dry-run --json` reporter. Restoring it was: parse two more flags, branch the inserter into a dry-pass (which already existed for the conflict counter), serde_json the report. The hard part was rediscovering that an early commit had the report shape correct and we could cargo-cult it back.

---

## 4. Bugs I suspect or noticed but wasn't asked to fix

File-level. No long descriptions.

1. **`crates/datum-core/src/field_spec.rs::regenerate_derived_aliases`** + **`db/alias.rs`**: `DerivedAliasKind::State` aliases collide silently across fields. If two specs share a state name (`done`, `active`, `idle`), the first saved wins and the loser is just *gone* from the alias table. No warning at write time, no surface in `dm check`. Easy bug, ugly UX.
2. **`crates/datum-core/src/field_spec.rs` terminal-end namespace collision**: `<field>e` is reserved for "end <field>". A field named `codee` (because the user has a `code` field) silently collides — its bare-field alias is shadowed by the terminal-end alias of `code`. Not currently checked at install time.
3. **`crates/datum-commands/src/cmd.rs::log_entry`** at the `args.last_state.is_some() && matches!(verb, Verb::Stop)` site (~line 593): `-L` is silently ignored on non-Stop verbs. The CLI accepts the flag for any verb. Either accept it or reject it; today it is accepted-and-dropped. (See also `user_asserted_last_state` in `db/insert.rs:657`.)
4. **`crates/datum-core/src/schema.sql::entries.raw_input`** has `NOT NULL DEFAULT ''`. The import bulk path I touched in ndd.10 never sets it for legacy CouchDB rows. They land with `raw_input = ''`. `alias_chain` is correctly `NULL`, but the `raw_input` empty string is a false negative for "we have user-source-of-truth" — `dm check --raw_input` would think we do.
5. **`crates/datum-commands/src/config.rs::data_dir` vs `config_dir`**: data_dir reads `XDG_DATA_HOME`, config_dir reads `XDG_CONFIG_HOME`. If a user sets only one, the DB and the rc-file split locations. Documented behaviour, but layered config feels more atomic than it is.
6. **`crates/datum-core/src/db.rs::migrate()` + `db/insert.rs` hid retry**: The hid-collision retry in `try_insert` retries once. The bulk import path has its own copy. Two implementations, one contract — when the policy changes (retry twice? probabilistic backoff?) we'll change one and not the other.
7. **`crates/datum-core/src/migrations.rs::m_19_to_20`**: the down migration is `NotReversible`. Roundtrip tests for v20→v19→v20 are technically green because the test harness skips the value-preservation assertion when `NotReversible` is declared. That's correct, but the *number* of irreversible migrations has crept up (m2→3 dropped entries, m_19→20 destroys hids, others). Pre-v1 should audit "does the user lose data on a downgrade?" per migration and document.
8. **`crates/datum-core/src/db/insert.rs::derive_verb_for_storage`**: the `parser_version` column is set to `1` on every insert and never read. Quartz noted this; I'll add — it's set in the bulk path *and* the trickle path *and* in tests as a literal `1`. When something does start reading it, every site has to be touched.
9. **`crates/datum-cli/tests/tail_cmd.rs::t19_watch_live_updates`**: skeleton only, `#[ignore]`d. The watch loop has no integration test that exercises an actual append→redraw. It's exercised manually. (Yes — by me. That is not coverage.)

---

## 5. One non-obvious improvement (the only section that matters)

> **Stop materializing derived aliases. Make `aliases` a user-only table; derive on read.**

This is invisible to anyone who didn't write the migration that introduced `derived_from_field`.

### What's there now

`field_spec.rs::regenerate_derived_aliases` walks a `FieldSpec` and emits a `Vec<DerivedAlias>`. On every spec save (`db/spec.rs`):

1. `DELETE FROM aliases WHERE derived_from_field = ?spec_field`
2. `INSERT INTO aliases (name, expands_to, derived_from_field, kind) VALUES …` — bare (`<field>` → `<verb> <field>`), terminal-end (`<field>e` → `end <field>`), and one row per `allowed_states` entry (`<state>` → `switch <field> <state>`).

For a spec with 8 states that's 10 row writes. For a kit with 30 specs, that's ~300 row writes per kit install.

At the input-pre-pass site (`expand_argv_with_db`), we read `aliases` and treat both kinds identically. The user types `happy`; the lookup hits `aliases` and finds either the user's hand-written alias *or* the spec-derived state alias. Same lookup path, same expansion logic.

### Why this is wrong

1. **Hot writes for cold reads.** Spec installs are infrequent. Alias lookups happen every CLI invocation. We're paying write cost in the wrong direction.
2. **State-name collisions are silent.** Two specs with `done` race on save order. The loser's alias is overwritten. The user has no idea their `done` doesn't expand the way they expect.
3. **`alias_chain` carries names that may be stale.** When a spec is renamed (say `mood` → `feeling`), all `Verb::Switch` entries previously logged via the alias `happy` carry `alias_chain = ["happy → switch mood happy"]`. The aliases table updates; the entries don't. We can't re-derive accurately.
4. **`dm check --aliases` has to reconcile two sources of truth.** Today there is no such command — and the reason it doesn't exist is partly that "what's the canonical truth?" has no single answer. Specs say one thing, materialized rows say another, the entries' `alias_chain` says a third.
5. **Two write paths, two failure modes.** The materialization in `db/spec.rs` uses one transaction; user-defined alias writes (`alias_cmd.rs`) use another. The unique-constraint on `aliases.name` means a user-defined alias can lose a write race against a spec save and the user sees `UNIQUE constraint failed: aliases.name` for an alias they didn't think conflicted.

### What I'd build instead

A two-layer lookup, with `aliases` carrying only user-defined entries:

```rust
// In datum-commands::expand pass:
fn expand_one(token: &str, db: &Db, spec_index: &SpecAliasIndex) -> Option<Expansion> {
    // 1. User-defined wins (so override is possible)
    if let Some(a) = db.lookup_user_alias(token)? { return Some(a.into()); }
    // 2. Derived from FieldSpec — built once per process from `field_specs`
    spec_index.lookup(token)
}
```

`SpecAliasIndex` is a small `HashMap<&str, Expansion>` built lazily on first use, invalidated on `field_specs` write. Conflicts (two State aliases for `done`) become a *load-time* warning instead of a silent overwrite. The `aliases` schema loses the `derived_from_field` column entirely — back to user-only.

Win: write cost on spec install drops from ~10×N to ~0 (the spec row itself). Lookup is O(1) hashmap hit. Conflict detection moves from "save order" to "deterministic, every load". `alias_chain` rot becomes a knowable artifact (the index can answer "is this alias_chain still valid?" per entry).

### Why quartz won't propose this

Quartz's retro is the whole-codebase view. The cmd.rs / parse.rs / verb-sweep agenda is the right top-three. Aliases-as-materialized is a specific failure mode I see *because I built the materialization*. It looks fine from outside. Nobody complains about it because the rare collisions are filed as user-confusion bugs, not alias-system bugs. The fix is a localized refactor (one table change, one read-path swap, one warning emitter) that pays back invisibly.

### Order of operations

1. Add `SpecAliasIndex` and a feature flag (`DATUM_DERIVE_ALIASES_ON_READ=1`) so both paths can run side-by-side for a release.
2. Run a `dm check --aliases-divergence` scan over real DBs; surface differences between materialized rows and what the index would compute. If divergence is non-empty, fix it before flipping the flag.
3. Migration drops the `derived_from_field` column and DELETEs all rows where that column was set. Make the `aliases` table user-only at the schema level.
4. Delete `regenerate_derived_aliases` and its callers in `db/spec.rs`. Replace with a single call to `SpecAliasIndex::invalidate()`.

This is a one-week move for a polecat who knows the territory.

---

## 6. My area's relationship to the whole

### Schema and aliases are a coupling hub. I made it tighter on purpose.

`schema.sql` ndd.10 changes ripple to: `db.rs`, `db/insert.rs`, `db/lookup.rs`, `db/spec.rs`, `db/alias.rs`, `entry.rs`, `migrations.rs`, every `tests/*_cmd.rs` that hard-codes hid prefixes, and the import bulk path. That's by design — the entry shape *is* the data model and there's no point pretending otherwise. But it means a schema change is never one PR; it's a slice through every layer above it.

### `datum-commands` is a coupling hub by definition

I built it that way — every CLI verb routes through `dispatch` in `cmd.rs`, every command file imports `args::LogArgs`, output and check live there because they're cross-command. The boundary I drew (datum-cli is the binary, datum-commands is the layer) does what it should: the CLI binary is small (`main.rs` is mostly argv pre-pass + dispatch), the layer is testable. The cost is that cmd.rs is the one place where everything meets. R2 (the actual carve-up) is the move that turns the hub into a constellation.

### `field_spec.rs` is a leaky abstraction

The spec types are pub-everywhere because the layout layer reads `y/height/z_index/relative_to`, the parse layer reads `default_verb` and `id_keys`/`data_keys`, the alias system reads `allowed_states`, the render layer reads `color`/`state_colors`. Every consumer has its own slice. There is no `FieldSpecPublic` interface — the struct *is* the interface. When I added `derived_aliases` and `kit_id` columns, every consumer who pattern-matched the struct compiled fine because Rust doesn't require exhaustive struct destructuring for `pub` fields. That's a footgun for type-driven refactors.

### `config.rs` is a clean leaf

The one piece of my work that's just a leaf. Callers take `String` paths and `Source` annotations; nobody round-trips through `Config` itself except `datum config show`. If we ever needed to rip it out, we could.

### `tail` is a feature, not a hub

The tail surface (`--watch`, `-L`, `--dry-run`) lives at the edges. It reads from db, writes nothing important, can be replaced. Good shape. The `-L/--last-state` work *did* leak into the storage path (`user_asserted_last_state` column) — that's the part that isn't a leaf. If we ever decide last-state assertion is per-block-edge metadata rather than per-entry, that column has to migrate.

---

## Closing — a builder's view from the schema seat

I'm the polecat who does the moves that look invisible in the diff and are loud in the bug tail. The R1 split is the loudest one — it shipped with R2 implied and R2 hasn't followed. The slug-as-PK pivot worked but doubled the number of code paths that have to remember "hid retry once." The auto-alias system shipped as materialized when it should have been derived-on-read.

Three things I'd change about my own contributions if I had them back:

1. **R1 should have been four PRs, not two.** Step 1 (create crate, move three commands) was right. Step 2 should have been *one verb at a time*, each PR moving real argv parsing into the per-command file, not a bulk shim-creation that left 66 functions bailing. The current shape implies "the work is half done" when really "the work is one tenth done and the placeholders are misleading."
2. **`derived_from_field` column should never have been added.** The right move on da-ndd.6 was the SpecAliasIndex above. I took the local fix because it was an evening's work, not the right fix because it was a week's. Pre-v1 should rip it.
3. **`m_19_to_20` should have been three migrations**, not one: rename `id` → `hid` semantically (m19→20), add `slug` index (m20→21), drop the legacy ULID validation (m21→22). The all-in-one migration is `NotReversible` because all three changes lump together; doing them in slices would have preserved value-roundtrip on at least the first two.

The codebase is in good shape. The shape it isn't in is *honest* — placeholders that look like progress, columns that look read-only, tables that look authoritative when they're cached. Pre-v1 cleanup is mostly the work of telling the truth in code.

— jade, 2026-05-06
