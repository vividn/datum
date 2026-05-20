# RETROSPECTIVE — daydumb engineering observations

**Author:** polecat quartz (rig: daydumb)
**Date:** 2026-05-05
**Workspace HEAD:** `polecat/quartz/da-71vm@mot2xmiw`, off `main` at `3338f35`
**Scope:** the whole `crates/*` workspace (10 crates), the integration tests, schema, and dev loop. Numbers below are from `wc -l` / `grep -c` over the tree at HEAD.

I've spent recent cycles on small, surgical fixes (da-5z01 edge-commas in switch state-lists, da-lvvp leading/trailing commas in field array-split, da-438y the missing-color path, da-6ymd the trailing +/- forced-future tokens). That's the perspective: not the architect who drew the lines, but the polecat who keeps tripping over them while changing one byte at a time.

---

## 1. What's working well

### Migrations are unusually disciplined
`crates/datum-core/src/migrations.rs` (2462 LOC) defines `Migration { from, to, summary, apply, down }` and walks an ordered table from version 1 to **CURRENT_SCHEMA_VERSION = 20**. Every step has both an up and a down, plus per-version round-trip tests in `crates/datum-core/tests/migration_roundtrip.rs` (691 LOC) and a chain test in `migration_chain.rs`. This is the part of the codebase I trust most. When I land a schema change, the chain test catches it the moment the migration table is wrong. Most projects this size never get this right; daydumb did.

### Bespoke parser is fast and predictable
`crates/datum-parse/src/parse.rs` (2641 LOC, 73 fns) is a hand-rolled argv parser — no clap. That decision pays dividends every invocation: the CLI starts in <5ms and the parser is wasm-clean (the crate builds for `wasm32-unknown-unknown` with `--no-default-features`). It's also why the web app and the CLI can share one `Cli` intent shape. The cost is real (see §2) but the choice was right.

### Integration test density at the CLI boundary
`crates/datum-cli/tests/` has **44 integration test files** including `sync_e2e.rs` (1068 LOC, real-server multi-instance), `db_import.rs` (775), `add_cmd.rs` (764), `tail_cmd.rs` (686), `switch_cmd.rs` (558), `conflicts_cmd.rs` (558). 1,347 `#[test]` functions across 122 files in the workspace. When the suite passes, the CLI behaves. The bug surface I keep hitting is *underneath* the integration layer — see §3.

### Verb derivation as a pure function
`crates/datum-core/src/verb.rs::derive_verb(state, dur, fs)` is pure, table-driven, and called from a single place at write time (`db/insert.rs::derive_verb_for_storage`). The da-5ns time-shape pivot (sub-beads .1–.8 in the log) introduced this and it's a model of how to do an invasive refactor: pick a derivation, store it for query speed, write a `dm check --verbs` to catch drift. I don't worry about verb correctness when I edit the storage layer because of this.

### Local-first SQLite, no daemon
One file, `rusqlite::Connection`, WAL mode. No daemon, no network for the hot path. `datum tail` is instant. The web app consumes the same schema via OPFS+WASM. Architecture stays small.

### Parity audits exist on disk
`docs/parity-audit/{commands,parsing,sync,views}.md` exist as living deltas vs. the original datum. Most rewrites in the same family don't bother — daydumb has receipts. da-8rnt's `docs/dependency-audit.md` (407 transitive packages, 0 advisories) is similarly real.

### Kit system is a coherent feature
`crates/datum-core/src/kit.rs` (1329 LOC) + `kit_color.rs` + `kit_color_rule.rs` (636) ship a TOML bundle format covering field specs, aliases, SQL views, Level-1/Level-2 kit commands, signed-bundle trust gate, with end-to-end roundtrip tests in `crates/datum-commands/tests/kit_roundtrip.rs` and `chores_kit.rs`. It's also the only place where a clean plugin model is plausible at v1.

---

## 2. What's gnarly

### `crates/datum-commands/src/cmd.rs` — 5498 LOC, 93 functions
This is the single most painful file in the workspace and the one I touch on almost every fix. It contains `dispatch`, `log_entry`, `cmd_tail`, `cmd_count`, `cmd_ids`, `cmd_info`, `cmd_setup`, plus a long tail of helpers: `parse_log_args`, `parse_switch_state_token`, `validate_switch_state_token`, `expand_array_states`, `parse_value_token`, `extract_reserved_columns`, `extract_add_temporal_keys`, `parse_reserved_occur_time`, `parse_add_time_value`, `check_no_time_flags`, `build_time_with_precision`, `capture_lead_state`/`capture_trail_state`, etc. Editing one means scrolling thousands of lines past unrelated code and re-running the whole crate's incremental rebuild.

The `commands/` directory next to it (40 modules: `add.rs`, `occur.rs`, `switch.rs`, `tail.rs`, …) reads like the cure but isn't. Every one of those files is a ~10-line shim:
```rust
pub fn occur(args: LogArgs, db: &Db) -> Result<()> {
    crate::cmd::log_entry(Verb::Occur, args, db)
}
pub fn occur_from_argv(_argv: &[String], _db: &Db) -> Result<()> {
    anyhow::bail!("occur_from_argv: stub — R2 extracts bespoke parser")
}
```
There are **66 R2-stub functions** that all `bail!`. The "module-per-command" layout is cosmetic. The R2 carve-up never started.

### `crates/datum-parse/src/parse.rs` — 2641 LOC, 73 functions, one mega-match
Each verb has its own `parse_<verb>_args` (`parse_log_args`, `parse_tail_args`, `parse_get_args`, …, `parse_dayview_args` at 1726, `parse_nowview_args` at 1886). Adding a flag to `tail` and `now` means editing two parser functions, possibly `data_args.rs`, the help spec at `help/commands.rs` (1027 LOC), and `args.rs` for the typed shape. The recent edge-comma bugs (da-5z01, da-lvvp) lived in *different* tokenizers in `cmd.rs::parse_switch_state_token` vs. `input/data_args.rs` array-split — the same kind of bug shows up twice because the tokenizer logic is duplicated.

`flag_registry.rs` exists in `datum-parse/src/input/` but the parser doesn't drive from it — it's currently descriptive, not authoritative.

### Eleven `#[cfg(any())]` test modules
Permanently false-gated test trees (this evaluates to "never compile"):
- `crates/datum-core/src/db/tests.rs` (2564 LOC) — gated by da-5ns sub-bead 1, "sub-bead 8's test sweep" was supposed to rewrite. Sub-bead 8 (4a847c8) shipped, the file is still gated. **2,564 lines of cold storage.**
- `crates/datum-core/src/conflicts.rs:405`
- `crates/datum-core/src/test_util.rs:444`
- `crates/datum-core/src/views/{sub_human_id,timing,state_change,duration_sum}.rs` — each module's local tests
- `crates/datum-core/src/ids/resolve.rs:242`
- `crates/datum-core/src/migrations.rs:{1909,1959,2229}` — three internal test groups

This rots silently. The CI sees green and the suite is one third smaller than it appears. **Either delete these or rewrite them. Cold storage is worse than absence — it tricks the next reader.**

### `db/mod.rs::migrate()` runs schema.sql twice with hand-patches in between
```rust
let _ = self.conn.execute("ALTER TABLE aliases ADD COLUMN derived_from_field TEXT", []);
let _ = self.conn.execute("ALTER TABLE aliases ADD COLUMN kit_id TEXT", []);
let _ = self.conn.execute("ALTER TABLE field_specs ADD COLUMN kit_id TEXT", []);
let _ = self.conn.execute("ALTER TABLE field_specs ADD COLUMN tags TEXT NOT NULL DEFAULT '[]'", []);
let _ = self.conn.execute("ALTER TABLE field_specs ADD COLUMN render_json TEXT", []);
// ... legacy ULID detection ...
self.conn.execute_batch(include_str!("../schema.sql"))?;
crate::migrations::migrate_to_current(&self.conn)?;
self.conn.execute_batch(include_str!("../schema.sql"))?;
```
The comment is honest about why ("schema.sql executes first and would error before `migrate_to_current` ever sees the DB"), but this is a defence-in-depth made of duct tape. Pre-v1 should fold these columns into `schema.sql` and drop the pre-flight ALTERs.

### Dependency direction is inverted vs. names
`crates/datum-core/src/lib.rs` ends with `pub use datum_parse::time;`. The "core" data-model crate re-exports `time` from `datum-parse`. Reasonable in the abstract (`datum-parse` is the wasm-clean lower crate) but the names lie: a stranger reading the workspace expects core → parse, and core's lib.rs admits the opposite. Either rename `datum-parse` to something honest (`datum-pure`?) or move `time` into `datum-core` and let `datum-parse` depend on it.

### `datum-cli/src/lib.rs` is 100% re-exports
```rust
pub use datum_commands::args;
pub use datum_commands::cmd;
pub use datum_commands::commands::check;
pub use datum_commands::commands::sync;
pub use datum_commands::config;
pub use datum_commands::render as output;

pub mod repl;
```
The whole reason `datum-cli` is a library is to keep test imports unchanged after R1's commands extraction. Every fix that adds a public symbol to `datum-commands` and uses it from a `datum-cli` test has to either re-export it here or import from the new path. The shim was meant to be temporary; nothing is more permanent.

### `dayview` and `nowview` are duplicated across crates
`commands/dayview.rs` (1521 LOC) + `layout/dayview.rs` (1013) + `layout/dayview_svg.rs` (670). `commands/nowview.rs` (1166) + `layout/nowview.rs` + `layout/nowview_svg.rs` + `layout/nowview_terminal.rs` (554). The split between command-layer and layout-layer is conceptually right but the seams aren't clean — each layer redoes context-building work. da-bop and da-6jy were both "reuse the dayview render path"; we're still consolidating.

### `Verb` enum carries legacy variants
`crates/datum-core/src/entry.rs::Verb` has `Point/Switch/Block/BlockSwitch/PointSwitch` *and* `Occur/Start/Stop/Add`. The doc comment says "Sub-bead 8 sweeps these out alongside the test rewrite." Sub-bead 8 happened (4a847c8). The legacy variants are still live and `derive_verb_for_storage` (`db/insert.rs`) special-cases `Verb::Stop` and `Verb::Start` for state canonicalization. The type system models a state of the world that no longer exists.

---

## 3. Test gaps

### Tokenizer edge cases
The bug pattern that hit me most this week: **leading/trailing/empty commas in lists**. da-5z01 (switch state-lists in `cmd.rs::parse_switch_state_token`), da-lvvp (field array-split in `input/data_args.rs`). These were two independent fixes in two files because the tokenizer logic is duplicated. There is **no property-based test** anywhere in the workspace (no `proptest` / `quickcheck` dep). A 50-line proptest over `","-separated tokens with optional whitespace and edges` would have caught both, plus the next three I'll find.

### `db/tests.rs` is gated off
The 2564-line internal test suite for the db layer is `#[cfg(any())]`. Today the only tests of the db layer are CLI integration tests, which means a refactor that breaks an isolated `Db::list_entries` invariant only surfaces if some CLI flow happens to exercise it. Most don't.

### Sync agent tests require live setup
`crates/datum-sync-agent/tests/daemon_relay_integration.rs` has 4 `#[ignore]`'d tests requiring a running relay + two DB roots. CI never runs them. da-bra's `sync_e2e.rs` is one 1068-line monolith; if a new wire-protocol message lands without explicit coverage, the suite still passes.

### Parser fuzz
`parse.rs` has 73 hand-rolled functions. Most have local unit tests in `parse.rs` itself, but combinatorial fuzz over flag interactions (`-yy`, `-q`, `-t -8`, trailing `+/-`, `=` form for hyphen values, `--alias <NAME>`) is missing. da-6ymd (trailing +/- forces future/past) is exactly the class of feature that interacts with `-t`/`-d` precedence in surprising ways — covered by example tests, not by combinations.

### Migration *data preservation*, not just *shape*
`migration_roundtrip.rs` proves shape can be walked up and down. Several down-migrations are explicitly destructive (m2_to_3 drops the entries table). There's no contract test that says "after up + down, user-visible entries are recoverable". For pre-v1 that's tolerable; locking v1 should add it.

### Render output goldens
`crates/datum-commands/tests/render_fixtures.rs` (695 LOC) exists for terminal output but `#[ignore]`s one test, and there's no PNG/SVG golden-image testing. da-glb / da-zxs / da-7r4 / da-25l / da-yiy — all visual fixes that were caught by eye, not by suite.

### `db_import` regression coverage
`db_import_regression.rs` exists, good. But fast-path `migrate import` (da-6l0, da-8p1, da-1el) had three follow-on fixes (da-7sr "drop premature derive_verb"). The bulk-import path is harder to test because it bypasses `try_insert`. It's the right fast path; it's also where most surprises will surface.

---

## 4. Architecture observations

### Top-level layering: clean on paper, leaky in practice
The intended stack is `datum-parse → datum-core → datum-layout → datum-commands → datum-cli`, plus the `datum-sync-{protocol,client,server,agent}` quartet hanging off the side. The audit:

- **Clean:** `datum-sync-protocol` is genuinely standalone (extracted in da-6hn.1).
- **Clean:** `datum-layout` is wasm-clean — PNG rasterization (`resvg`) is pulled in by `datum-commands`, not the layout crate (good call, intentional).
- **Leaky:** `datum-core` re-exports `datum_parse::time`. Names lie.
- **Leaky:** `datum-cli` lib is purely re-exports of `datum-commands`. The "datum-cli is the binary, datum-commands is the layer" boundary is enforced by intent only.
- **Leaky:** `datum-commands` depends on `datum-sync-client` directly, and on `resvg`. The "command layer" carries a network client and a font rasterizer in its dep tree. Both have justifications, but it means `cargo build -p datum-commands` is not lightweight.

### Module-per-feature vs. function-per-feature ratio
- **Module-per-feature** (good): `crates/datum-core/src/db/` (15 submodules), `crates/datum-core/src/views/` (4 view types + mod helpers), `crates/datum-parse/src/input/` (data_args, time_args, duration_args, …), `crates/datum-parse/src/time/`, `crates/datum-parse/src/alias/`.
- **Function-per-feature** (bad): `crates/datum-commands/src/cmd.rs` (5498 LOC, 93 fns), `crates/datum-parse/src/parse.rs` (2641 LOC, 73 fns), `crates/datum-core/src/migrations.rs` (2462 LOC, 51 fns).

The codebase knows how to do it right. Three holdouts dominate the pain.

### Responsibility cuts are blurry in three places
1. **`datum-parse` vs. `datum-core` for time.** Time parsing/types live in `datum-parse`; `datum-core` re-exports. Argument: `datum-parse` had to be wasm-clean and time was the dependency that needed it. Counter: time is a model concept; argv-parsing is an input concern; they should live apart.
2. **`datum-commands::render` vs. `datum-layout`.** Render is in `commands` (terminal table, info-line). Layout is its own crate (dayview/nowview SVG/PNG). The boundary is "render = string output, layout = pixel output" — fine, but `nowview --terminal` blurs it (terminal output of the nowview computation lives in `datum-layout::nowview_terminal.rs`, 554 LOC).
3. **`field_spec.rs` (1674 LOC in core) vs. `db/spec.rs`.** Spec storage and spec semantics are both in core but split; saving a spec touches both. The `DerivedAlias` lifecycle (BareField, TerminalEnd, State; first-saved-wins for State) is documented in `field_spec.rs` and enforced by code paths in `db/alias.rs`. The contract isn't surfaced in types.

### Persisted intent — good, but thread-local state is a smell
`raw_input`, `alias_chain`, `parser_version` columns store user-source-of-truth so we can re-derive parsed shapes after grammar changes. Right call. But `cmd.rs::log_entry` reads its `IntentContext` off a thread-local set during pre-pass, with the comment: "must be captured *before* `inner` runs". REPL mode (introduced da-lgf) will eventually expose this when commands run multiple times in one process; future async will too.

---

## 5. Schema / API debt — what to lock down before v1

Concrete list. Not "consider improving" — what I'd cut.

1. **Drop `Verb::{Occur,Start,Stop,Add}`.** The new five-verb set is canonical. Sweep the `Display` / `FromStr` for `note` → `Verb::Add`, `end` → `Verb::Stop`. Pick `note` *or* `add`, not both; pick `stop` *or* `end`, not both.
2. **Delete `EntryShape`.** Its own doc says "compatibility shim ... value is derived from `dur` rather than stored." It hasn't earned its slot in the public API.
3. **Type the "ignored" state.** `IGNORED_STATE_VALUE` is a stringly-typed sentinel. Make it a `State::Ignored` variant or wrap `state` in a typed enum at the boundary. Spec docs already say "tracking state is NOT stored on the spec; it derives from the entry log via `Db::state_at`" — fine, but the derivation reads a magic string.
4. **Fold the `migrate()` ALTER hacks into `schema.sql`.** `derived_from_field`, `kit_id`, `tags`, `render_json` — and the legacy ULID detection — were duct tape for in-flight migrations. Cut them at v1.
5. **Drop `parser_version` until it's actually read.** It defaults to 1 on insert and nothing reads it. Reserve the name in the schema, leave it out of the type, OR wire it up. Schrödinger's column.
6. **Decide on `dur` representation.** It's TEXT in the DB (ISO 8601), `Option<String>` on the struct. Most callsites parse it back. Either keep it as a string and remove the parsing duplication (one canonical parser at the boundary), or store it as INTEGER seconds with a string adapter for compat.
7. **Retire `OutputFlags::fresh()` (legacy flat shape).** Phase D was supposed to retire it for the spec/alias commands. Three commands still use it. Either ship Phase D or admit it's permanent and drop the deprecation comment.
8. **Lock `extra_metadata` as system-only via type, not docs.** Today the contract is "Hidden from default render paths — only `-A`/Show::All exposes it." Enforce via a wrapper newtype or by routing `datum edit` through a path that physically can't touch `extra_metadata`.
9. **Remove the `DerivedAliasKind::State` "first saved wins" silent collision.** Surface a warning at write time and a `dm check --aliases` repair step. Today specs that share state names quietly clobber each other depending on save order.
10. **Pick a slug-collision policy.** da-p5j said "slug collision must error; hid retry once" — good. The implementation is split across `db/insert.rs` and `db/lookup.rs`. Centralize.
11. **`raw_input NOT NULL DEFAULT ''` is a surrender.** A non-empty `raw_input` is the contract. The default-empty exists because some import paths don't have raw input. At v1, decide: synthesize one, or make the column nullable and stop pretending.

---

## 6. Tooling / dev-loop friction

1. **No CI configured at the rig level.** Search of `.github/`, `*.yml` outside `docker-compose.yml` yields nothing. Quality gates are: developer machine + Refinery merge queue. There's no automated `cargo test`, `cargo fmt --check`, or `cargo clippy` between commit and merge.
2. **No clippy gate, no `clippy.toml`.** Lint warnings accumulate silently.
3. **`cargo test` runs everything including `sync_e2e.rs`.** No `--lib` separation, no test profiles. `sync_e2e.rs` spins a real `datum-sync-server`; a polecat doing one-line fixes pays for the whole pyramid.
4. **`xtask` is underused.** It does manpages + readme. Lint-level invariants belong here:
   - Fail if `#[cfg(any())]` appears in `crates/`.
   - Fail if `anyhow::bail!("...stub — R2 extracts...")` exists.
   - Fail if `Verb::Occur` / `Verb::Start` / `Verb::Stop` / `Verb::Add` is referenced.
   - Fail if a file in `crates/` exceeds N LOC (3000?). cmd.rs and parse.rs hardcode a "rewrite never finishes" budget.
5. **Single-file changes in `cmd.rs` recompile all of `datum-commands`.** 5498 LOC is one compilation unit. Splitting cmd.rs into per-verb modules (real ones, not the R2-stub shells) wins back incremental compile budget.
6. **`serde_yaml` is deprecated upstream** (per da-8rnt audit). Replace before v1. `datumrc.yml` is small; YAML→TOML is straightforward and we already depend on `toml`.
7. **No `cargo nextest` in the toolchain.** With 1,347 tests, the speedup is real.
8. **Test fixtures duplicated.** `datum-core/src/test_util.rs` (513 LOC) + `crates/datum-cli/tests/common/` + `crates/datum-cli/tests/cli_harness.rs` + `crates/datum-cli/tests/fixtures/` — fixture builders exist in three layers. Consolidating to `test-util` features per crate would cut duplicates.

---

## 7. Cross-cutting bugs I'd file beads for

In file-and-line detail, no long descriptions:

1. **`crates/datum-core/src/db/mod.rs::migrate()`** runs `schema.sql` twice; refactor so it runs once *after* migrations (or only on fresh DBs). Future ALTERs will surprise.
2. **`crates/datum-cli/src/main.rs::try_kit_level2_dispatch`** re-runs `expand_argv_with_db` after a parse error, doubling DB hits and possibly running an alias twice. Merge with the primary parse path.
3. **Tokenizer duplication for comma-separated lists** between `crates/datum-commands/src/cmd.rs::parse_switch_state_token` and `crates/datum-parse/src/input/data_args.rs` field array-split. Same edge-comma bug shape twice.
4. **`crates/datum-core/src/entry.rs::Verb` legacy variants** (Occur/Start/Stop/Add) — sub-bead 8 advertised the sweep, didn't sweep.
5. **`#[cfg(any())]` test modules (11 sites)** — delete or rewrite. Cold storage masquerading as tests.
6. **`crates/datum-commands/src/commands/*.rs` R2-stub bodies (66 fns)** — they don't run, they don't help, they make the directory look richer than it is.
7. **`OutputFlags` (legacy flat shape in `datum-parse/src/cli.rs`)** — Phase D abandoned mid-flight; pick "ship it" or "delete the deprecation comment".
8. **`crates/datum-core/src/migrations.rs::down_2_to_1_*`** — destructive down-migrations don't have data-loss tests; the contract is by docstring only.
9. **Color-resolution paths smeared across `cmd.rs` and `render/`** — da-bop, da-8mu, da-438y all fixed *one* path, missing siblings. Unify into a single `resolve_color(field, context, surface) -> Rgb`.
10. **Thread-local `IntentContext` in `cmd.rs::log_entry`** — non-reentrant. Will bite REPL when REPL grows beyond one-shot mode.
11. **`datum-cli` lib.rs re-export shim** — costs every public-symbol change a touch in two crates. Stop pretending it's transitional.
12. **`serde_yaml` deprecated** — replace with `serde_yml` fork or drop YAML config (`datumrc.yml` → `datumrc.toml`).

---

## 8. Things that make daydumb great — preserve through any refactor

1. **<5ms startup is religion, not a target.** The original datum's 1s startup was the entire reason for the rewrite. Anything that bloats `main()` — async runtime, lazy_static initialization, network calls in the hot path — must be rejected on sight. The blocking `reqwest` (`default-features=false`, `blocking,json,rustls-tls`) is documented as deliberate; that pattern should be the rule.

2. **Datum-compatible time syntax: `-y`, `-yy`, `-q`, `-qq`, `-t -8`, `-t 1513`, `-t 9:15am`, trailing `+/-` (da-6ymd), compact `apr29` (da-x1cw), `-d -1`, `--dur 30m`/`PT30M`/`1:30`/`2h`.** Every one of these is in `crates/datum-parse/src/time/`. They are why a datum user can switch and feel home. Lock them in tests; never break them silently.

3. **The verb model.** `occur` (point), `switch` (transition), `start`/`stop` (block edges), `note`. It maps to how people actually narrate their lives. The (state, dur, following_state) triple is the right normal form behind it. Keep both — don't collapse the verb into a derived-only field; users type verbs, machines store triples.

4. **Local-first SQLite, single file.** Backup is `cp`. Restore is `cp`. Web app uses the same schema via WASM+OPFS. No "cloud is the source of truth" lie. The sync-server is content-blind and stays that way.

5. **E2E-encrypted sync with content-blind relay.** `datum-sync-protocol` is its own crate so the wire is reviewable in isolation. AES-GCM at the boundary, recovery passphrase via Argon2 wraps the data key, server stores opaque base64. WebAuthn (passkeys) for auth — no passwords. This is real, not theatre.

6. **Migration discipline.** 20 schema versions, every one with up + down + roundtrip test. Rare. Keep adding `down` for every `up` even when destructive — the `NotReversible` error type is the right escape valve.

7. **Kit format as the plugin model.** TOML field specs + aliases + Level-1/2 commands + SQL views + signed-bundle trust. Solves "how do power users share configuration" without a runtime extension language. da-d4t.1–.7 stayed scoped; preserve the seven-slice cadence next time.

8. **Parity audits as a discipline.** `docs/parity-audit/{commands,parsing,sync,views}.md` exist and are kept current. Most rewrites of "spiritual successor of X" don't write down what they kept and what they dropped. Daydumb does. Keep doing it.

9. **`dm check`, especially `--verbs`, `--fix`.** Tools that diagnose drift between stored and re-derived data are how you build trust in invasive refactors. Add `dm check --aliases`, `dm check --colors`, and ship more of them.

10. **The fact that intentions are persisted** — `raw_input`, `alias_chain`, `parser_version`. When the parser changes, we can re-derive. Most projects throw away the user's literal text the moment they parse it. Daydumb doesn't.

---

## Closing — a polecat's view

The codebase is in late-pivot shape. The big moves are done (da-5ns time pivot, da-9l1 db split, da-6hn sync extraction, da-d4t kit slices) and what remains is the unsexy carve-up of `cmd.rs` and `parse.rs`, the deletion of legacy verb/shape variants, and the closing of test gaps. I'd put it at "85% of the way to a v1 that ages well, 15% from a v1 that becomes the next thing to apologize for."

Three single-PR moves I'd take before locking v1:

1. **Cut the 11 `#[cfg(any())]` tests.** Either rewrite or delete. Cold storage rots.
2. **Sweep the legacy `Verb` variants.** Sub-bead 8 promised it; do it.
3. **Add a proptest harness for the comma/whitespace tokenizers.** It catches the next two bugs before I find them by hand.

If I were given autonomy for one week without a backlog, I'd take a chainsaw to `cmd.rs` and turn the 40 R2-stub `commands/*.rs` files into actual command modules with their own argv parsers — the one move that buys the most future-self goodwill.

— quartz, 2026-05-05
