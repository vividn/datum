# Commands — Parity Audit (daydumb vs datum)

This document audits every CLI command in **daydumb** (Rust) against the
original **datum** (TypeScript, `/tmp/datum-survey`). For each command we
record the feature surfaces on both sides, call out divergences, note
missing behaviors, and flag tests that should be ported. Written for R1
of the refactor epic (`da-t6t.1`); R2 (bespoke parser) and R3 (views
rewrite) will reshape several of these commands further.

Reference points:
* daydumb handlers: `crates/datum-commands/src/cmd.rs` (functions
  `cmd_<name>` or `log_entry` for occur/start/stop/switch/add).
* daydumb args: `crates/datum-commands/src/args.rs`.
* datum source: `/tmp/datum-survey/src/commands/<cmd>Cmd.ts`.
* datum tests: `/tmp/datum-survey/src/commands/__test__/<cmd>Cmd.test.ts`.

---

## Summary of surface deltas

| Command | datum has | daydumb has | both | added in daydumb |
|---|---|---|---|---|
| add / occur / start / end / switch | ✓ | ✓ | ✓ | `-K` slug keys, `-m` moment, auto-alias expansion |
| tail / head | ✓ | ✓ | ✓ | `--reverse` |
| get / del / edit | ✓ | ✓ | ✓ | `--force` on del, `--pretty` on get |
| update / merge / rekey | ✓ (one cmd, `-X` flag) | ✓ (three cmds) | ✓ | split into separate verbs |
| retime | ✓ | ✓ | ✓ | stricter mutual-exclusion on `--offset`/`--time` |
| backup / restore | ✓ | ✓ | ✓ | tar.gz archive support |
| grep | ✓ | ✓ | ✓ | `-f` field filter |
| map / reduce | ✓ | ✓ | ✓ | — |
| dayview | ✓ | ✓ | ✓ | `--width`/`--height` |
| now | ✓ | ✓ | ✓ | — |
| check | ✓ (with `--fix`) | ✓ (`--json`) | ✓ | structured `--json` report |
| migrate | ✓ | ✓ | run/status/edit/import | — |
| setup | ✓ | ✓ | ✓ | bare migrate-only (purge moved to `db purge`) |
| sync | ✓ | ✓ | both exist, **different protocols** | OAuth + E2E key mgmt |
| spec | ✗ | ✓ | — | field schemas in DB |
| alias | ✗ | ✓ | — | DB-backed CLI aliases |
| track / ignore | ✗ | ✓ | — | tracked-fields registry |
| info | ✗ | ✓ | — | multi-DB health view |
| config | ✗ | ✓ | — | `datumrc.toml` introspection |
| conflicts | ✗ | ✓ | — | D5 conflict queue (see `da-gnj.5`) |
| history / undo / why | ✗ | ✓ | — | EntryHistory trail (see `da-gnj.4`) |
| v1 | ✓ (legacy) | ✗ | — | not carried forward |

Tests ported: the `db2_parser`, `d4_history`, `conflicts_cmd`, and the
per-verb tests under `crates/datum-cli/tests/` cover the daydumb side.
Most datum `Cmd.test.ts` suites have **not** been mechanically ported.

---

## Per-command audit

### add

- **Original datum:** freeform entry; inherits newDocArgs (`--no-metadata`,
  `--id`, `--id-delimiter`, `--conflict`, `-u/--undo`, `-U/--force-undo`,
  `-x/--merge`); full timeArgs (`-d`, `-y`, `-t`, `-q`, `-z`, `-D`,
  `-T`); `-k key[=default]` for data keys.
- **Daydumb today:** `log_entry(Action::Add, ...)` with `LogArgs`. Infers
  the (state, dur, following_state) triple from `start_time=`, `end_time=`,
  `dur=`, `state=` keys; `derive_verb` then classifies into the v3 storage
  verb (`point` / `block` / `block-switch` / `point-switch` /
  `switch`). `-K` slug-contributing keys and `-k` data-only keys are
  split (daydumb enhancement). `-m/--moment`, `--last-state`,
  `--conflict`, `--update`, `--merge`, `-M`, `-z` timezone, `-u` unit,
  `-c` comment, `--id`, `--id-delimiter`.
- **Missing in daydumb:** no `-u/--undo` / `-U/--force-undo` flag on the
  log verbs — `undo` is its own command.
- **Divergences:** daydumb always records `alias_chain` + `raw_input`
  (da-gnj.3); datum does not. daydumb's auto-alias generation from spec
  states (da-gnj.3) has no datum equivalent. Unit annotation is captured
  via `{"v": x, "u": "unit"}` in `extra_data`; datum stores bare values.
- **Tests in datum not in daydumb:** the 25 tests in `addCmd.test.ts`
  (doc creation, undo/force-undo, metadata suppression, custom id
  templates, data parsing variations) are not mechanically ported;
  daydumb coverage is in `add_cmd.rs` / `d2_parser.rs` / `d3_auto_alias.rs`
  integration tests (covers a subset).
- **Ideas / cleanup:** consider a `--undo` short-circuit that records
  the reversal in EntryHistory rather than running a separate verb, to
  close the UX gap.

### occur

- **Original datum:** time args (`-d/-y/-t/-q/-z/-D/-T`), data args,
  newDocArgs. Point-in-time entry (`occurTime` only).
- **Daydumb today:** `log_entry(Action::Occur, ...)` using `LogArgs`.
  Stores `occur_time` only (no `dur`). State is left null; `derive_verb`
  classifies (state=None, dur=None, fs=None) → `Verb::Point`. `DatumTime
  { utc, offset, tz, precision }` captures uncertainty (d2). The v=20
  schema's `occur_time_precision` column carries an ISO-8601 duration
  (e.g. `PT5M` for ±5 minutes), replacing the v=19 (seconds, unit) pair.
- **Missing in daydumb:** `--no-timestamp` (`-T`) flag has no surface
  equivalent — omitting a time always produces `now()` in daydumb.
- **Divergences:** daydumb tracks **precision** (day / minute / second)
  as an explicit `Uncertainty` column; datum stores precision implicitly
  via truncation.
- **Tests in datum not in daydumb:** 9 tests (timezone round-trip,
  offset notation, occurTime storage, LastState dedup).
  `occur_cmd.rs` covers the happy path.
- **Ideas / cleanup:** add a `--omit-timestamp` flag to record an entry
  with no time at all (for abstract data where time is meaningless).

### start

- **Original datum:** positional duration, or `-d/--dur`; inherits
  timeArgs + data args.
- **Daydumb today:** `log_entry(Action::Start, ...)` with `LogArgs`.
  Action canonicalization sets state=true on entries that don't supply
  one explicitly. `--dur` sets `dur = ISO 8601 duration` from
  `time_start` forward; without `--dur` the row's storage verb derives
  to `Verb::Switch` (state=set, dur=null). With `--dur` it derives to
  `Verb::Block`.
- **Missing in daydumb:** positional duration — daydumb requires the
  `--dur` flag.
- **Divergences:** none of note.
- **Tests in datum not in daydumb:** 10 tests (duration formats, time
  offsets, timezone). Covered by `start_cmd.rs`.
- **Ideas / cleanup:** consider accepting a bare trailing duration
  argument in the bespoke parser (R2) for datum-compatibility.

### end / stop

- **Original datum:** reuses `startArgs`; symmetrical closer.
- **Daydumb today:** `log_entry(Action::Stop, ...)` with `LogArgs`.
  Action canonicalization sets state=false. `--last-state` closes the
  most recent open block and records its prior state; errors if no
  open block exists. `--dur` sets `time_start = time_end - dur`
  (backward direction). Bare stop derives to `Verb::Switch`; stop with
  `--dur` derives to `Verb::Block`.
- **Missing in daydumb:** nothing significant.
- **Divergences:** daydumb's `--last-state` error is explicit; datum
  silently skips or guesses when no open block.
- **Tests in datum not in daydumb:** 8 tests (retroactive close,
  prior-state capture). Covered by `end_cmd.rs`.
- **Ideas / cleanup:** `stop` and `end` share `end.rs` — consider
  collapsing to `end` only in user docs and keeping `stop` as a hidden
  alias for R2.

### switch

- **Original datum:** field positional, state positional, `-L/--last-state`,
  `--moment`, duration/time/data args.
- **Daydumb today:** `log_entry(Action::Switch, ...)`. `-m/--moment`
  produces a zero-duration entry (`occur_time == computed_end_time`,
  storage verb `Verb::Point`). `--last-state` dedupes when new state ==
  current (errors on redundant transitions). State normalization via
  `datum_core::state::normalize_state`. Without `--dur` derives to
  `Verb::Switch`; with `--dur` derives to `Verb::Block`; with `--dur`
  + `following_state=` derives to `Verb::BlockSwitch`.
- **Missing in daydumb:** explicit manual prior-state override (datum's
  `-L`).
- **Divergences:** daydumb errors on no-op transitions; datum accepts
  them silently. daydumb auto-generates aliases for spec states (e.g.
  `datum mood happy` → `datum switch mood happy`) — no datum equivalent.
- **Tests in datum not in daydumb:** 16 tests (normalization, moment,
  last-state manual). `switch_cmd.rs` covers core cases.
- **Ideas / cleanup:** expose a `--last-state=<value>` override matching
  datum's `-L` to smooth the migration path.

### tail

- **Original datum:** field optional, `-n`, `-m/--metric`, `-w/--watch`,
  `--column` (repeatable), timeArgs.
- **Daydumb today:** `TailArgs` — field optional, `-n`, `-m/--metric`,
  `-r/--reverse`, `--column`, `-d/-t/-y/-q` time bounds.
- **Missing in daydumb:** **no `--watch` / live-update mode.**
- **Divergences:** daydumb adds `--reverse` to invert order (default is
  newest-first). Column splitting supports comma-separated values
  (datum: one flag occurrence per column).
- **Tests in datum not in daydumb:** 19 tests; watch-mode tests can't
  port (behavior not implemented). `tail_cmd.rs` covers the read-once
  paths.
- **Ideas / cleanup:** `--watch` is a genuine feature gap; file a
  follow-up bead. SQLite `WATCH` via `sqlite3_update_hook` or polling
  loop would work.

### head

- **Original datum:** same flag surface as tail, reversed semantics.
- **Daydumb today:** `HeadArgs` — field, `-n/--limit`. Simpler than tail.
- **Missing in daydumb:** metric selection, column customization, time
  bounds.
- **Divergences:** daydumb's head is a stripped-down tail; datum's is
  a tail with reversed flag.
- **Tests in datum not in daydumb:** 16 tests (shared with tail via
  helper). daydumb `head_cmd.rs` is minimal.
- **Ideas / cleanup:** fold `head` into `tail --oldest-first` for R2 and
  delete the separate handler.

### get

- **Original datum:** `quickId` positional (supports `_` for last,
  wildcards). Output args.
- **Daydumb today:** `GetArgs` — `id_or_slug` positional, `--pretty`.
- **Missing in daydumb:** `_` / wildcard quickId resolver.
- **Divergences:** daydumb is strict on id/slug lookup.
- **Tests in datum not in daydumb:** 4 tests (last-entry shortcut, slug
  disambiguation).
- **Ideas / cleanup:** add a quickId resolver in R2 for datum parity.

### del (delete)

- **Original datum:** `quickId` positional, output args. Supports batch
  via quickId wildcards.
- **Daydumb today:** `DeleteArgs` — `id_or_slug`, `--force` (skip
  confirmation).
- **Missing in daydumb:** batch delete via wildcards, no confirmation
  prompt UX.
- **Divergences:** daydumb asks for confirmation (unless `--force`);
  datum does not prompt (`quickId` is the gate).
- **Tests in datum not in daydumb:** 6 tests (soft-delete flag, undo/
  restore, batch).
- **Ideas / cleanup:** quickId batch support is a genuine UX gap; file
  as a follow-up.

### edit

- **Original datum:** `quickId` + output args. Opens `$EDITOR`.
- **Daydumb today:** `EditArgs` — `id_or_slug` + output flags.
- **Missing in daydumb:** nothing significant; `TooManyToEditError`
  type is not replicated.
- **Divergences:** daydumb emits a generic error on multi-match rather
  than a typed error.
- **Tests in datum not in daydumb:** 3 tests (overwrite, single-entry
  constraint).
- **Ideas / cleanup:** consider streaming the editor diff through
  EntryHistory on save (da-gnj.4 hook).

### update / merge / rekey

- **Original datum:** **one command** `update`, with `-X/--strategy`
  selecting update / merge / deep-merge. `quickId` positional.
- **Daydumb today:** **three commands** (`update`, `merge`, `rekey`);
  `UpdateArgs` shared — `id_or_slug`, trailing positional payload,
  `--strategy`, `--id` (rekey), `--id-delimiter`, `-k/--key`, `-c/--comment`.
- **Missing in daydumb:** single-entry-point mental model.
- **Divergences:** daydumb splits into three verbs because each has a
  different default and `rekey` additionally takes `--id` template
  flags. datum uses "deep-merge" wording; daydumb uses `merge` (deep
  in extra_data) distinct from `update` (shallow replace).
- **Tests in datum not in daydumb:** 11 tests — most strategies are
  covered by daydumb `update_cmd.rs` + `merge_cmd.rs`; batch
  quickId-based mutations are a gap.
- **Ideas / cleanup:** keep the three-verb split; R2 should document
  the mapping in `--help` text.

### retime (rt)

- **Original datum:** `-o/--offset`, `-t/--time`, `-f/--field`,
  `--after`, `--before`.
- **Daydumb today:** `RetimeArgs` — `slug` optional, `--offset`,
  `-t/--time`, `-f/--field`, `--after`, `--before`. `--offset` and
  `--time` are **mutually exclusive** (enforced).
- **Missing in daydumb:** nothing.
- **Divergences:** stricter validation on flag combinations. `--after`
  / `--before` accept both RFC3339 and `YYYY-MM-DD`.
- **Tests in datum not in daydumb:** 8 tests (offset direction, batch
  retime). `retime_cmd.rs` covers the core behavior.
- **Ideas / cleanup:** the uncertainty re-derivation (d2) is daydumb-
  specific and lives here — worth documenting in the command's `--help`.

### backup (datum db backup)

- **Original datum:** `filename` positional, `--overwrite`. JSONL only.
- **Daydumb today:** `BackupArgs` — `path`, `--overwrite`. Lives under
  `datum db backup` (da-33g). Detects `.tar.gz` / `.tgz` and emits a
  gzipped tar archive; otherwise JSONL.
- **Missing in daydumb:** `backupTime` in JSONL header — daydumb emits
  raw JSONL lines, one per entry.
- **Divergences:** archive support is daydumb-only. Restore detects
  the same suffixes.
- **Tests in datum not in daydumb:** 4 tests. `backup_cmd.rs` covers
  the JSONL path; tar.gz paths tested via `backup_tgz_cmd.rs`.
- **Ideas / cleanup:** add a JSON metadata header to the JSONL variant
  (matching datum's `backupTime`) so restore can detect schema version.

### restore (datum db restore)

- **Original datum:** path positional, output args. JSONL only.
- **Daydumb today:** `RestoreArgs` — `path`, `--force` (maps to
  `--conflict update`). Lives under `datum db restore` (da-33g).
  Auto-detects tar.gz.
- **Missing in daydumb:** nothing significant.
- **Divergences:** `--force` is daydumb-only; datum relies on entry
  content-hash to dedupe.
- **Tests in datum not in daydumb:** 4 tests. `restore_cmd.rs` covers
  JSONL + tar.gz paths.
- **Ideas / cleanup:** wire up a `--dry-run` to mirror the behaviour
  we already give `db import`.

### grep

- **Original datum:** `patterns` (repeatable positional, AND logic).
- **Daydumb today:** `GrepArgs` — `patterns` (required, repeatable),
  `-f/--field` filter. Output flags.
- **Missing in daydumb:** nothing.
- **Divergences:** `-f/--field` is daydumb-only.
- **Tests in datum not in daydumb:** 1 minimal test. `grep_cmd.rs`
  exercises the happy path.
- **Ideas / cleanup:** expose a `-v/--invert` flag (datum doesn't have
  it, but it's a natural grep-ism).

### map

- **Original datum:** `view_name` positional, `--key`, `--startkey`,
  `--endkey`, `--group`, `-g/--group-level`, `--limit`.
- **Daydumb today:** `MapArgs` — same surface. Emits
  `{key, value, slug}` JSON per row (slug is the source entry's PK).
- **Missing in daydumb:** **reduce is a separate command** — no
  `--reduce` flag on map.
- **Divergences:** see reduce.
- **Tests in datum not in daydumb:** 0 dedicated tests (views tested
  separately).
- **Ideas / cleanup:** R3 rewrites views in SQL with indices — this
  command's shape will change significantly.

### reduce

- **Original datum:** same flag surface as map; `--reduce` flag folded
  in.
- **Daydumb today:** `cmd_reduce(args, db)` — forces `reduce=true` via
  `map_args_to_params(args, true)`.
- **Missing in daydumb:** nothing.
- **Divergences:** daydumb splits map vs reduce into separate commands.
- **Tests in datum not in daydumb:** 0.
- **Ideas / cleanup:** collapse back into `map --reduce` in R3.

### dayview

- **Original datum:** `--date`, `--output`, `--json`. Fixed SVG
  dimensions.
- **Daydumb today:** `DayviewArgs` — `--date`, `--output`, `--json`,
  `--width` (default 2000.0), `--height` (default 200.0).
- **Missing in daydumb:** nothing.
- **Divergences:** daydumb exposes size controls.
- **Tests in datum not in daydumb:** 1 minimal test.
- **Ideas / cleanup:** honour terminal size when `--output` is stdout
  so `datum dayview | less` still renders.

### now (nowview)

- **Original datum:** `-A/--show-all` to include ignored fields.
- **Daydumb today:** `NowArgs` — `-A/--show-all`. Colored field display
  via spec metadata.
- **Missing in daydumb:** nothing.
- **Divergences:** daydumb uses per-field color from `datum spec set
  --color` (stored in DB); datum uses a hard-coded palette.
- **Tests in datum not in daydumb:** no dedicated file.
- **Ideas / cleanup:** expose `--no-color` for pipe-friendly output.

### check

- **Original datum:** `field` optional, `--fix` (auto-repair), output
  args.
- **Daydumb today:** `CheckArgs` — `--json`. No auto-fix.
- **Missing in daydumb:** `--fix` / auto-repair.
- **Divergences:** daydumb separates **detection** (check) from
  **resolution** (conflicts resolve / conflicts auto-resolve, see
  da-gnj.5). datum combines them with `--fix`.
- **Tests in datum not in daydumb:** 0 baseline tests; daydumb's
  `check_cmd.rs` exercises all current categories.
- **Ideas / cleanup:** a `--fix` shim that delegates to
  `conflicts auto-resolve` would close the gap without re-merging the
  detect/resolve paths.

### migrate (datum db migrate)

- **Original datum:** `run`, `status`, `edit`, `import` subcommands.
  import: `--from`, `--backup`, `--dry-run`.
- **Daydumb today:** `MigrateCommand` with `run`, `status`, `edit` —
  schema-migration only, dispatched under `datum db migrate` (da-33g).
  The legacy `import` verb moved to `datum db import` (da-fab); see the
  `db` section. `cmd_migrate_edit` is a stub (open `$EDITOR` against the
  migration file).
- **Missing in daydumb:** interactive migration editing is just a
  `$EDITOR` launcher.
- **Divergences:** schema version check during `run` refuses to proceed
  if the binary is older than the stored schema. `import` lives under
  `db` (DB-admin verbs), not `migrate` (schema migrations) — semantic
  split landed via da-33g.
- **Tests in datum not in daydumb:** no dedicated file; the legacy
  `db import` path is exercised by `db_import.rs` /
  `db_import_regression.rs` and one acceptance check in
  `migrate_cmd.rs`.
- **Ideas / cleanup:** `status` could print the list of pending
  migrations with descriptions, not just a version range.

### setup (datum db setup)

- **Original datum:** no flags; bare `datum setup` initialises PouchDB.
- **Daydumb today:** bare `datum db setup` (da-33g) — applies any
  pending schema migrations and stamps the version. The destructive
  rebuild moved to `datum db purge` (da-e16); the `--force-recreate`
  alias was removed in da-57t.
- **Missing in daydumb:** nothing.
- **Divergences:** `db purge` is daydumb-only.
- **Tests in datum not in daydumb:** none material; idempotency and
  newer-schema rejection covered in `info_setup_cmd.rs`, destructive
  rebuild covered by `db_cmd.rs`.
- **Ideas / cleanup:** none.

### sync

- **Original datum:** custom protocol for CouchDB → CouchDB replication.
- **Daydumb today:** `SyncCommand` with `login`, `config`, `key`
  subtrees. OAuth device-code flow + E2E-encrypted relay server
  (`crates/datum-sync-server`) + AES-256-GCM master-key lifecycle
  (`init` / `unlock` / `rotate-passphrase` / `show-fingerprint`).
  The client library lives in `crates/datum-sync-client`; a future
  `datum-sync-agent` daemon will wrap it for background sync.
- **Missing in daydumb:** nothing at the feature level — daydumb's
  sync is a superset.
- **Divergences:** **completely different protocol.** No wire-level
  parity. The relay server is content-blind.
- **Tests in datum not in daydumb:** sync integration tests exist on
  both sides but are protocol-specific.
- **Ideas / cleanup:** R4 (da-t6t.4) split the monolithic `datum-sync`
  crate into `datum-sync-client`, `datum-sync-server`, and a
  `datum-sync-agent` stub — see `docs/parity-audit/sync.md`.

### spec (daydumb only)

- **Purpose:** field-schema management stored in the SQLite DB (no
  datum equivalent).
- **Subcommands:** `set`, `show`, `list`, `remove`.
- **Notes:** specs drive auto-alias generation, default verbs, state
  color palettes, unit defaults, slug templates, and id-key shapes.
  See `da-gnj.3` for the full spec extension set.

### alias (daydumb only)

- **Purpose:** DB-backed CLI alias management (not shell aliases).
- **Subcommands:** `set`, `show`, `list`, `remove`.
- **Notes:** aliases expand during the pre-pass before clap sees argv.
  `datum alias set vbp "note word srcLang=pt pos=verb"`.

### track / ignore (daydumb only)

- **Purpose:** registry of which fields appear in `datum now`.
- **Notes:** track can override per-field or per-state colors. Ignore
  hides a field without losing its tracked status permanently.

### info (daydumb only)

- **Purpose:** multi-DB health/metadata view.
- **Flags:** optional `target` (name or path), `--all` (iterate
  registry + discovered DBs).
- **Notes:** prints schema version vs binary, entry counts, tracked
  field counts, last-write time, file size. Missing DBs render as
  `✗ (missing)` without aborting.

### config (daydumb only)

- **Purpose:** introspect `datumrc.toml` + env + built-in defaults.
- **Subcommands:** `show` (with `--json`).
- **Notes:** rows include per-key source (default / toml / env / flag).
  No write path — the config file is hand-edited.

### conflicts (daydumb only, D5)

- **Purpose:** D5 conflict queue — list / resolve / auto-resolve.
- **Subcommands:** `list`, `resolve`, `auto-resolve`.
- **Notes:** three conflict kinds (`double_open`, `orphan_end`,
  `overlap`). The auto-resolver has a `--min-age` safety window.

### history (daydumb only, D4)

- **Purpose:** show the modification trail (EntryHistory).
- **Flags:** `id_or_slug`, `--version`, `--diff`, `--json`.
- **Notes:** every write goes through EntryHistory (da-gnj.4); history
  is the inverse of `undo`.

### undo (daydumb only, D4)

- **Purpose:** revert an entry to a prior version (or undo a delete).
- **Flags:** optional `id_or_slug`, `--to-version`, `--force`,
  `--window` (default `15m`).
- **Notes:** the safety window prevents surprise reverts of old data.

### why (daydumb only, D3)

- **Purpose:** show provenance — raw_input + alias_chain.
- **Flags:** `id_or_slug`, `--full` (trace the full history trail).
- **Notes:** the intent context is captured at write time (da-gnj.3)
  so `why` is a DB read, not a parse.

### v1 (datum only)

- **Purpose:** legacy v1 entry shape normalization.
- **Daydumb:** **not ported.** Legacy entries come in via `datum
  db import --from <backup.json>` which normalizes on ingest.

---

## Testing gap summary

Datum commands with substantial test suites that have **no direct
mechanical port** in daydumb (covered incidentally by integration
tests):

| Command | datum tests | daydumb coverage |
|---|---|---|
| add | 25 | `add_cmd.rs` + parser / alias suites |
| switch | 16 | `switch_cmd.rs` — subset |
| tail | 19 | `tail_cmd.rs` — no watch-mode tests |
| head | 16 | `head_cmd.rs` — minimal |
| update | 11 | `update_cmd.rs`, `merge_cmd.rs` — subset |
| start | 10 | `start_cmd.rs` |
| occur | 9 | `occur_cmd.rs` |
| end / stop | 8 | `end_cmd.rs` |
| retime | 8 | `retime_cmd.rs` |
| delete | 6 | `delete_cmd.rs` — no batch tests |
| setup | 4 | `setup_cmd.rs` |
| restore | 4 | `restore_cmd.rs` |
| backup | 4 | `backup_cmd.rs` + `backup_tgz_cmd.rs` |
| get | 4 | `get_cmd.rs` |
| edit | 3 | `edit_cmd.rs` |
| dayview | 1 | — |
| grep | 1 | `grep_cmd.rs` |
| v1 | 0 | N/A (not ported) |

Total: ~150 datum tests; daydumb currently passes 400+ workspace tests
(including non-command coverage: parser, views, spec, sync, history,
conflicts). Porting the remaining datum-specific behaviors (quickId
wildcards, `--watch`, batch mutations) is the main follow-up.

---

## Top refactor follow-ups

1. **`--watch` for tail.** Genuine UX gap. Worth a dedicated bead.
2. **QuickId resolver.** `_` for last, wildcards / substring match.
   Currently only exact id or slug is accepted.
3. **Batch mutations.** `del`, `update`, `merge`, `rekey` should accept
   a quickId that matches multiple rows.
4. **JSONL backup header.** Carry a `backupTime` sentinel so restore
   can validate schema version.
5. **Head/tail unification.** R2 candidate — collapse into one command.
6. **Map/reduce unification.** R3 candidate — same underlying view.
7. **Command `--help` text alignment.** Every command's help should
   call out daydumb-specific flags that have no datum equivalent
   (`-K`, `--reverse`, the auto-alias behavior, ...).

---

*Last touched against `main` @ `4db4903` on 2026-06-02 (vremya/parity-audit-footer). Drift is expected between touches; if a claim here disagrees with code, the code wins. To refresh: walk the doc against current `main`, update the items, bump this footer's sha + date in the same commit.*
