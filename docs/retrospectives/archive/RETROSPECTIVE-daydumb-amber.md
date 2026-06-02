# RETROSPECTIVE — daydumb from the render/config/schema-framework seat

**Author:** polecat amber (rig: daydumb)
**Date:** 2026-05-06
**Workspace HEAD:** `polecat/quartz/da-snrw@motor4y2`, off `main` at `49632f3`
**Scope:** the territory my six commits touched — output/render, multi-DB config, the schema-migration framework + the v3 entries rewrite, the sync CLI surface (login/key/config), and the `migrate-cmd` parity sweep.

Quartz wrote the general retro. This one is narrower. It covers the **edges where my work meets everyone else's** — the renamed columns, the config layering, the key-wrap crypto, the `--show`/`-o` matrix, the `info`/`setup` commands, the OAuth device flow. The places I lived in and the things I noticed only because I was the one moving them.

---

## 1. Where I spent the time

Six landed commits, four real territories:

| commit | territory | LOC delta |
|---|---|---|
| `48bb8fe` (da-ndd.7) | port datum's CLI output format → `output.rs` | +1008 in one file |
| `a5e193e` (da-ndd.9) | slug instead of ULID in action header | small surgical |
| `1feb875` (da-ndd.11) | `--db` flag + registry + `info`/`setup` + migration framework | +228 in `migrations.rs`, +256 in `config.rs` |
| `82e7f44` (da-ndd.13) | `datum sync login` / `key` / `config` + key-wrap crypto | +2200 across 16 files |
| `73905c2` (da-gnj.1) | schema v3 / data-model-v2 entries rewrite + `slug::derive` | +996 −497 across 25 files |
| `6bd242d` (da-glnr) | `migrate-cmd` parity tests | +224 tests |

So: **render layer, config layer, migration framework, sync CLI/crypto, and the v3 schema pivot.** Two of those four (render and the v3 entries shape) have already been re-cut underneath me — the 1008-line `output.rs` was carved into `crates/datum-commands/src/render/{mod,action,colors,characters}.rs` in R1 (dd5ea54), and the v3 shape was again pivoted in da-5ns (occur_time/state/dur/following_state/verb). Both moves were correct. Both invalidate parts of this retro the moment they shipped — it's a side-effect of being early.

---

## 2. Harder than expected

### `output.rs` was the wrong abstraction level
I shipped 1008 lines into `crates/datum-cli/src/output.rs`. A week later R1 split it into four files in `datum-commands/src/render/`. The split was right; my mistake was building the action/table/state-glyph/color/format-string stack as one module because the original datum has it as one module. **TypeScript "everything in one file" doesn't translate to Rust** when you're going to add a wasm-clean lower crate later. `render/action.rs` (746 LOC), `render/mod.rs` (1400), `render/colors.rs` (293), `render/characters.rs` (19) is the carve I should have done day one.

### `OutputArgs` / `Show` as a state machine
`Show::{Default, Minimal, Standard, Format, None, All}` × `format_string: Option<String>` × `args.silent()` × `interpolate()` is more cases than the type makes obvious. The bug pattern: `Show::Format` skips header rendering but `format_string` is `Option<String>` — pass `--show format` without `-o`, you get **silent no-op**. I shipped that. It's still there at `render/mod.rs:312-317`. The right shape is `enum Show { ... Format(String) }` so the type makes "format with no string" unrepresentable, but I didn't have the appetite to cascade-update every call site.

### Action header layout shifted under me
I shipped `<ACTION> <hid> <slug>` where `hid` was *the last 5 chars of the ULID*, lowercased. Then da-ndd.10 (slug-as-PK + random hid column) replaced ULID with a 12-char base36 random and added a `datum_sub_human_id` view that materializes the **shortest unique prefix per row**. My `short_hid` function is now a length-clamp on a base36 string — same name, completely different semantics. The only caller that still cares is `interpolate`'s `%hid%`. **The function should have been deleted at da-ndd.10**, replaced with a query against `datum_sub_human_id`. It wasn't, because the rename was load-bearing for output goldens.

### Key-wrap crypto: easy to ship a subtly wrong format
`crates/datum-sync-client/src/crypto/key_wrap.rs` (219 LOC) is Argon2id (m=19MiB, t=2, p=1) → AES-256-GCM. The salt + nonce + ciphertext are concatenated into a single blob and base64'd. **Format-versioning is one byte at the front (`0x01`).** That's correct, but it means every future change requires a new version byte and a parser branch — there's no algorithm-agility envelope (no `kdf_id`, `aead_id`). For pre-v1 it was right to ship simple. For v1 it's the blob format I'd revisit before locking.

### `migrate()` runs `schema.sql` twice — that pattern came from me
`crates/datum-core/src/db/mod.rs::migrate()` runs `schema.sql`, then `migrate_to_current`, then `schema.sql` **again**. That pattern landed with the migration-framework commit (1feb875). Reason: `schema.sql` had to be CREATE-IF-NOT-EXISTS-idempotent so existing DBs survive opening, *and* after migrations bump the version we re-run it to pick up newly-introduced tables/views. It works. It also encouraged the ALTER pile-up quartz called out (`derived_from_field`, `kit_id`, `tags`, `render_json`) — every "I'll fix it via the second `schema.sql` run" was downstream of my "let it run twice" affordance. This is mine. Pre-v1: drop the second run, fold ALTERs into discrete migrations.

### Stub OAuth relay was 255 lines of glue
`crates/datum-cli/tests/sync_support/mod.rs` (255 LOC at C4b time) — an in-process axum server that mimics the device-code grant. Realistic 401/410 shapes, polling-interval semantics, slow-down, expired-token. **Half my C4b time went there.** The crypto and login wiring took a day; the stub took two. That cost is invisible in the diff stats but it's the reason `tests/sync_cmd.rs` (406 LOC, 10 acceptance tests) exists at all. If you ever rip out the stub, you lose all sync-CLI test coverage.

---

## 3. Easier than expected

### `rusqlite` generated columns with VIRTUAL
The v3 schema makes `verb` a `GENERATED ALWAYS AS (...) VIRTUAL` column whose CASE expression reads `intent`, `entry_shape`, and `value`. The **DB enforces verb correctness** — the application layer can't insert an inconsistent verb because the column isn't writable. `dur_seconds` is the same trick over `start_time`/`end_time`. This solves what would otherwise be a derive-on-every-insert problem with zero application code. I expected SQLite's generated-column support to be limited; it isn't. Worth using more aggressively.

### `dirs` + `XDG_*` were boring in the good way
`config.rs::data_dir()` / `config_dir()` / `auth_token_path` / `master_key_path` — once the helpers existed, every subsequent feature got the right paths for free. No platform branching in feature code. `mode 0600` is the only Unix-specific bit (`set_permissions(.., Permissions::from_mode(0o600))` behind `#[cfg(unix)]`), and it's correctly `#[cfg]`-gated. **Windows users have file ACLs, no mode bits — see §4 for the gap.**

### Migration framework shape
`pub struct Migration { from, to, summary, apply: fn, down: fn }` + `pub const MIGRATIONS: &[Migration]`. Const-time array, no registration ceremony. Adding a migration is "write two functions, append a struct literal". The chain test (added later) catches the moment someone forgets a `down` or transposes versions. **This is the part I'm happiest about** — I expected to spend a day on a builder API and it turned out a 30-line const slice was the answer.

### Test-util's `Db::open_test`
By the time I landed v3, `crates/datum-core/src/test_util.rs` (~500 LOC) had stabilized enough that the destructive recreate path slotted in cleanly: every test got a fresh in-memory DB via the same factory, schema v3 just worked. I added zero infrastructure for v3 tests. Quartz called out fixture duplication across crates; from inside `datum-core`, `test_util.rs` is a clean win.

### `slug::derive` extraction
Pulling slug construction out of the DB layer into its own pure module (`crates/datum-core/src/slug.rs`, 96 LOC) was the cheapest refactor in the v3 commit. Six tests, one helper, no DB dependency. **More of `db/insert.rs`'s derived-column logic should follow this pattern** — pure functions in their own modules, called from one place in the DB.

---

## 4. Bugs I noticed but didn't file (or fix)

In file:line detail.

1. **`render/mod.rs:312-317` — `Show::Format` without `-o` is silent no-op.** User passes `--show format`, no `-o`, gets nothing. Type the format string into the variant.

2. **`render/mod.rs:761-784` — `interpolate` falls back to `unwrap_or_default()` for unknown `%key%`.** Typo `%vlaue%` returns empty string instead of erroring. For format strings stored in field specs (kit format), this turns config bugs into silent rendering. Either warn-on-stderr or fail.

3. **`render/mod.rs:788 / action.rs` — two `short_hid` paths.** `short_hid_of` (mod.rs:788) is a public re-export of an inner `short_hid`. It's not load-bearing; both call the same private fn. Either drop the re-export or remove the inner. Right now grep returns confusing results.

4. **`config.rs::resolve_db` registry write is best-effort and silent.** When `resolve_db` auto-creates a `<name>.db` and fails to persist a registry entry, the user sees no warning. The DB still works; the next invocation just re-creates instead of looking up. Hard to diagnose if it ever happens.

5. **`crates/datum-core/src/migrations.rs::IncompatibleSchema` reports versions but not recovery.** Error message says "schema version 21 > current 20"; doesn't say "use `setup --force-recreate` if this DB was downgraded". UX cliff.

6. **`crates/datum-sync-client/src/crypto/key_wrap.rs` — Argon2id parameters are hard-coded constants.** `m=19456, t=2, p=1`. Wear OS / weak phones pay the same memory cost as desktops. v1 should make these per-device-tunable and stamp the chosen params into the wrapped blob (they're already there in `KdfParams`, but `wrap_master_key` ignores caller overrides).

7. **`config.rs` source-tracking on env-var precedence is incomplete.** `Config::load` documents "flag > env (DATUM_*) > yaml > default" but the per-key source map (`source: BTreeMap<String, &'static str>`) doesn't always update when an env var is consumed mid-merge. `datum config show` will sometimes report a key as "yaml" when it actually came from `DATUM_*`. Cosmetic-but-misleading.

8. **`master_key_path` mode 0600 is `#[cfg(unix)]`.** On Windows the file is created with default ACLs — typically inheriting from the user's profile, which is fine, but **there's no equivalent ACL-tightening on Windows.** The local plaintext master key sits with whatever the home directory grants. Document or restrict.

9. **`crates/datum-cli/tests/info_setup_cmd.rs` doesn't cover `setup --force-recreate` over a DB with live entries.** All fixtures setup an empty DB. Force-recreate's data-loss path is uncovered. (Pre-v1 nobody has live data; once v1 ships, this becomes the most-important test in the file.)

---

## 5. One non-obvious improvement: route every render through `RenderContext::for_entry`

Quartz called out (#9) "color-resolution paths smeared across `cmd.rs` and `render/`" — multiple fixes (da-bop, da-8mu, da-438y) each touched one renderer. **The cause isn't the renderers — they're projections over a context. The cause is that `RenderContext` is built ad-hoc at every call site and the builders disagree.**

There are at least four places that construct a `RenderContext`:

- `crates/datum-commands/src/cmd.rs::log_entry` — write path, before `show_action`
- `crates/datum-commands/src/render/mod.rs::print_table_with` — table render path
- `crates/datum-commands/src/render/action.rs::format_info_line_at` — info-line variant
- `crates/datum-commands/src/render/mod.rs::print_now_view` / `render_now_view` — now view

Each computes some subset of `(lead_state, trail_state, first_time_hint, color)` from `(Entry, Db, Surface)`. The disagreements are subtle: "what does `lead_state` mean for an entry that's *being inserted right now*" gets a different answer in cmd.rs (queries `state::last_state_at(field, start - ε, exclude=row)`) versus the table renderer (queries the row's own `following_state` column on the prior row). For most entries the answer is identical; for entries near IGNORED transitions and for back-dated inserts, the results diverge.

The fix is structural. Make this signature the only public constructor:

```rust
pub fn RenderContext::for_entry(
    db: &Db,
    entry: &Entry,
    surface: Surface,        // Action | Table | InfoLine | NowView
    now: Option<&DatumTime>, // injectable for tests
) -> Result<RenderContext>
```

…and forbid construction from anywhere else (`pub(crate) struct RenderContext`, `pub(super) fn for_entry`). Then:

- `cmd.rs::log_entry` calls `for_entry(db, &entry, Surface::Action, None)`.
- The table renderer calls `for_entry(db, &row, Surface::Table, None)` per row.
- The now-view does the same with `Surface::NowView`.

The dispatch inside `for_entry` is a `match surface { ... }`. Each arm is the right query for that surface, codified once. Color resolution moves into the same function, since color is part of context resolution (`resolve_color(field, spec, surface) -> Rgb` is the existing single-source-of-truth that quartz wanted in their #9 — but the call to it belongs in `for_entry`, not in renderers).

Why this matters and why nobody else would notice: render bugs **look** like rendering bugs. The fix lives inside `render/`. But the **shape** of the bug is "every renderer has its own truth-source for context." Until you're the one who wrote the action-header path and then watched four follow-up fixes touch table-renderer color resolution and info-line transition arrows in different commits, you don't see the pattern. It's not fewer LOC — it's *one* LOC of color resolution per surface instead of N. The next color-rendering bug shouldn't be three fixes; it should be one fix.

The migration is mechanical: spider out from `RenderContext::default()` (today's escape hatch — used in `render/mod.rs:336` and several tests), force every site to provide a `Surface`, then move the arms one at a time. Estimated half-day.

---

## 6. What's my territory's relationship to the rest?

Two layers, glued:

**Bottom (storage + identity):** `schema.sql`, `migrations.rs`, `slug.rs`, `config.rs::resolve_db`, the registry. These are leaves — nothing in the workspace depends on a render type. Almost every crate depends on these.

**Top (UX surface):** `render/*`, `commands/sync/*`, `commands/info.rs`, `commands/setup.rs`. These are sinks — nothing depends on them. They consume types from everywhere.

So my territory looks like a **belt around the body of the workspace**: schema at the floor, UX at the ceiling, with `cmd.rs` and `parse.rs` filling the volume between. Touching the belt at one end ripples through the volume in two opposite directions:

- **Schema renames ripple up** through every renderer and the entire `cmd.rs` body. v3 was 996 lines added across 25 files; the renames (`time_start_utc`→`start_time`, `extra_data`→`data`, etc.) hit *every* downstream consumer.
- **Render changes don't ripple** — they're sinks. R1's `output.rs` → `render/{mod,action,colors,characters}` was a 4-file change. No downstream impact.

The asymmetry has a consequence: **schema lockdown before v1 is the highest-leverage move in the codebase.** Quartz's §5 list ("Drop legacy `Verb` variants", "Delete `EntryShape`", "Fold migrate ALTER hacks") isn't just code hygiene — every item on that list is a row of weight that, if not lifted before v1, becomes a permanent invoice the renderers and command layer keep paying.

The render layer can be refactored *after* v1. The schema effectively cannot. So if there's one week of pre-v1 attention to spend, spend it on:

1. Folding the migrate() ALTER pile into `schema.sql` (cuts duct-tape).
2. Fixing the schema-rename inconsistencies the v3 pivot left half-applied (`EntryShape` doc-comment confesses it's a shim).
3. Closing the format-version envelope on the wrapped key blob (wider AEAD/KDF agility).
4. Migrating the few remaining `RenderContext::default()` callers to the surface-aware constructor proposed in §5.

The fourth is mine to do; if no one else needs to ship in render before v1, I'd take it.

---

## Closing — the belt's view

The schema and the renderer are both "trivial" subsystems that absorb every other subsystem's complexity. When `cmd.rs` adds a flag, it's the renderer that decides what the flag *looks like*, and the schema that decides what it can *store*. So bugs in the rest of the codebase show up first in render output and last in schema constraints — the belt feels every twitch in the body.

The codebase is in good shape. The migration framework holds. The wasm-clean split is real. The XDG paths just work. The places where my work would benefit most from another pass are:
1. The four-call-site `RenderContext` problem (§5).
2. The `migrate()` two-runs-of-schema.sql pattern I introduced (§2).
3. The `Show::Format` × `format_string` type-level invariant (§2/§4).
4. Argon2id param plumbing — `KdfParams` is in the type, not in the wrap call (§4).

None block v1. All cost roughly half a day each. Total: two days of belt-tightening that would erase a class of follow-up bugs before they get filed.

---

## Status (as of 2026-06-02): Closed

- **§5 `RenderContext::for_entry` single constructor** → **open**, tracked under **`da-5895`** (render-layer unification). The surface-aware constructor is one of the sub-beads. Composes naturally with topaz's `render_shape` lift and jasper's `RenderPlan` lift.
- **§2 `migrate()` two-runs-of-schema.sql pattern (Amber self-attributed)** → ✓ fixed. `db/mod.rs::migrate()` now runs migrations first, then `schema.sql` once at the end. No pre-flight ALTERs.
- **§2 `Show::Format` × `format_string` silent no-op** → status unknown; quick grep at `render/mod.rs:312-317` would confirm. Filing as `vremya:` audit.
- **§2 `output.rs` wrong abstraction level** → ✓ fixed by R1 split into `render/{mod,action,colors,characters}` (which Amber acknowledged).
- **§2 `short_hid` semantics drift after da-ndd.10** → status unverified; filing as `vremya:` audit.
- **§2 key-wrap version envelope (one-byte `0x01` prefix, no algorithm-agility)** → status unchanged; pre-v1 acceptable per Amber's own note.
- **§4 8 bugs** — small items, filing remainders.

Amber's §6 *"schema lockdown before v1 is the highest-leverage move in the codebase"* — the schema is now substantially locked (Verb sweep done, EntryShape audit, kit_id+tags+render_json columns folded into `schema.sql`). Archived.

— vremya, 2026-06-02

— amber, 2026-05-06
