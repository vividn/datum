# Parsing — Parity Audit (daydumb vs datum)

This document audits the argv-parsing and time-parsing layers of **daydumb**
(Rust) against the original **datum** (TypeScript, `/tmp/datum-survey/src/`).
It is the R2 companion to the R1 commands audit (`docs/parity-audit/commands.md`).

Reference points:

* daydumb parse (post-R2 layout): `crates/datum-parse/src/`.
* daydumb argv parsing (post step-3/N): `crates/datum-parse/src/parse.rs`
  + `cli.rs` (typed `Command` enum). Bespoke parser; no clap.
* daydumb alias/swap pre-pass: `crates/datum-parse/src/alias/`
  (`expand_argv`, `apply_terminal_verb_override`, `maybe_swap_field_verb`,
  `expand_stacked_flags`, `try_expand_alias`) — relocated from
  `datum-commands` in step 3/N.
* datum `src/input/`: argparse-backed argument groups, consolidation, and
  flexible positional handling.
* datum `src/time/`: luxon-backed parsing and display.

R2 migrates these layers into the `datum-parse/` crate so that (a) web/mobile
frontends can reuse the parser (WASM-compilable), (b) programs can drive the
CLI via a typed API *or* a string API with identical semantics, and (c) the
same `flexiblePositional` + `key=value`-anywhere flexibility datum enjoys
is preserved.

The R2 bead lands incrementally:

| Step | Scope | Status |
|---|---|---|
| 1/N | New `datum-parse/` crate; move `time.rs` → split `time/` submodules; `PrecisionUnit` moved from `datum-core::entry`; WASM build verified; **this audit** | **done** |
| 2/N | `datum-parse` input/ + alias/ module scaffold; port `consolidate_keys` + `flexible_positional` with full TS-parity tests; stubs for the rest pointing back at this audit | **done** |
| 2b/N | Port `data_args`, `time_args`, `duration_args`, `field_args`, `output_args`, `quick_id_arg` from TS + tests | **done** |
| 3/N | Replace clap in `datum-commands::args`; remove `clap` workspace dep; `CommandIntent` + bespoke `--help` in `datum-parse`; alias expansion migrates into `datum-parse::alias` | **done** (commit 74fac61, da-jzj — clap removed from the entire workspace) |

Everything below is what the final R2 target must cover. Where daydumb has
*not yet* ported a behavior, it is marked **PENDING** with the step that
should deliver it.

---

## Summary of surface deltas

| Area | datum has | daydumb has | both | added in daydumb |
|---|---|---|---|---|
| `flexiblePositional` | ✓ | ✓ (step 2/N) | ✓ | — |
| `consolidateKeys` (`-k`) | ✓ | ✓ (step 2/N) | ✓ | — |
| `-K` id-key variant | ✓ | ✓ (step 2b/N, `DataArgs::push_id_key`) | ✓ | — |
| `key=value` interleaved with positional | ✓ | ✓ (step 2b/N, `handle_data_args`) | ✓ | — |
| `--lenient` / `--strict` / `-R` / `-S` / `-C` remainder modes | ✓ | ✓ (step 2b/N) | ✓ | — |
| `-b/--base-data` (JSON object) | ✓ | ✗ | — | — |
| `-d` / `-t` / `-y*` / `-q*` / `-z` / `-D` / `-T` | ✓ | ✓ (datum-parse) | ✓ | `-yy`/`-qqq` shorthand via pre-pass; `±N[unit]` uncertainty suffix (D2) |
| `--dur` (duration) | `duration` positional | `--dur` flag | ✓ | explicit flag vs positional |
| `-m/--moment` | ✓ | ✓ | ✓ | — |
| field/verb order-swap | ✗ | ✓ | — | `datum <field> <verb>` ↔ `datum <verb> <field>` |
| alias pre-pass in DB | ✗ | ✓ | — | DB-backed CLI aliases (D3) |
| terminal-verb override (`<field> end` → `end <field>`) | ✗ | ✓ | — | — |
| spec-driven auto-alias | ✗ | ✓ (planned D3 auto-gen) | — | field-spec-derived |
| `--show` / `--show-all` / `-o` | ✓ | ✓ | ✓ | richer `Show` enum parsing |
| `quickId` positional | ✓ | ✓ | ✓ | — |

`shell-quote`-based string input (datum's `cliInput: string | string[]`
entry point) is covered in daydumb today by splitting at the caller (tests
pre-tokenize); step 2/N adds a first-class `datum_parse::parse_argv_string`
that tokenizes via a portable shell-style splitter.

---

## `input/` coverage — per file

### `consolidateKeys.ts` (17 LOC)

Consolidates repeated `-k` flags so that the last-seen form (required /
optional / default) wins in the first-seen position. Example:
`-k key -k another=value -k key=default -k third= -k another` collapses
to `-k key=default -k another -k third=`.

- **Original:** `splitFirst("=", key)`; linear scan, O(n²) but n is tiny.
- **Daydumb today:** clap's `Vec<String>` accumulator gives the raw list
  but **no consolidation** happens. This means a user who writes
  `-k key -k key=default` gets two entries in `LogArgs::keys` and the
  log handler walks them in order, which can misbehave when key names
  collide.
- **Target (step 2/N):** `datum_parse::input::consolidate_keys(Vec<String>) -> Vec<String>`.
- **Port strategy:** direct, single-pass over the vec using
  `split_once('=')`. The algorithm is tiny (see TS — a `reduce`).

### `dataArgs.ts` (269 LOC — the core)

The workhorse: turns the raw `data` positional list + `-k/-K/-R/-S/-C/-c
/-b/-l/--strict` flags into a final `DatumData` object. Key behaviors:

1. **Explicit `key=value`** consumed as the `key` with `value`.
2. **Positional** consumed against the *next unfilled* declared key.
3. **`-K` id-key** — same as `-k` but also appends `%keyName%` to
   `idParts` so the slug template picks it up.
4. **`-c/--comment`** — repeatable, joined into `comment` array.
5. **`-b/--base-data`** — a `relaxed-json` object merged at the bottom of
   the result.
6. **Remainder handling** —
   - `-R <key>` routes unrecognised positional values to `<key>` (array).
   - `-S/--string-remainder` joins remainder as space-separated string.
   - `-C/--comment-remainder` === `-SR comment`.
   - `-l/--lenient` makes `extraData` the default remainder bucket.
   - Without any of the above, extra data throws `ExtraDataError`.
7. **Default values** — `-k key=default` fills from default when the
   positional slot is missing OR when the user wrote `.` as a skip.
8. **Command-change escape** — positional `dur`/`start`/`stop` values can
   trigger a mid-parse command change (`changeDatumCommand`); the parser
   recurses with the remainder.
9. **Idempotence** — re-entering `handleDataArgs` with the returned
   namespace yields the same result (used internally when a command
   change happens).

- **Daydumb today:** all of the above is inlined into the log path in
  `datum-commands/src/cmd.rs` (`log_entry` / `build_data_args` —
  ~200 LOC). The implementation covers the common case but lacks:
  - `-K` id-slug wiring (ID structure is ad-hoc — slug templates
    handle it on write instead).
  - Explicit `-R/-S/-C/-l/--strict` parity. Daydumb always treats
    extras as `extraData` (implicit lenient).
  - `-b/--base-data` JSON seeding.
  - The command-change escape (`dur=5 start` → changes verb to
    `start` mid-parse). Daydumb chose a static verb-first dispatch.
- **Target (step 2/N):** `datum_parse::input::data_args` module with a
  `DataArgs` struct + `handle_data_args(&mut DataArgs) -> DatumData`
  returning `serde_json::Value` (matching the extra_data leaf layout
  with `{"v": x, "u": "unit"}` tagging).
- **Divergences to carry forward:** daydumb's `value` column is a
  first-class field; datum stored the primary value under a variant
  key in `data` (e.g. `state` for switch). Our `DataArgs` output needs
  to expose the primary separately — likely via an additional
  `primary_key: Option<String>` hint on the struct so the caller can
  split the result.

### `flexiblePositional.ts` (31 LOC)

Transforms a single named argparse arg into a positional key before
`handleDataArgs` sees it. Used by commands like `switch` (`state`
argument) and `occur` (`dur` argument).

- **Mechanism:** prepends the value onto `args.data`, prepends the
  key name onto `args.keys`. An `afterOtherArgs` flag pushes to the
  end instead.
- **Daydumb today:** each command's arg struct has a named field
  (e.g. `SwitchArgs::state`); the log path stuffs it into data_args
  inline. Works for the common cases but doesn't generalise — adding
  a new "flexible positional" means editing the struct AND the handler.
- **Target (step 2/N):** `datum_parse::input::flexible_positional::apply`
  that takes a `DataArgs` builder and a `(field, key_name, skip_key,
  after_other_args)` tuple. Ordering requirement ("call in reverse
  order") maps to Rust by having each command's intent-build step
  apply its positionals outermost-first.

### `mainArgs.ts` (202 LOC)

Top-level dispatch. Takes `cliInput: string | string[]` (tokenises via
`shell-quote` when a string), extracts the first positional as the
command name, hands remaining args to per-command dispatchers.

- **Daydumb today (post step-3/N):** `datum_parse::parse(argv)` returns
  a typed `Command` enum (one variant per subcommand). The pre-pass
  (`expand_argv` + `try_expand_alias` etc.) lives in
  `datum-parse::alias`. See `crates/datum-parse/src/parse.rs` +
  `crates/datum-parse/src/alias/`.
- **Command aliases in datum (built into the switch):**
  - `stop` → `end`
  - `del` → `delete`
  - `red` → `reduce`
  - `rt` → `retime`
  - `mig`/`migration` → `migrate`
  - `update`/`merge`/`rekey` → `updateCmd` with `strategy` flag
    (daydumb already split these into distinct subcommands; keep that).

### `timeArgs.ts` (138 LOC)

Argparse wrapper for the timing flags (`-d -y -t -q -z -D -T`).
`handleTimeArgs` composes them into a `DatumTime`.

- **Daydumb today:** every log-verb arg struct has these fields
  individually (`yesterday_count: u8`, `quick_count: u8`, `date`,
  `time`, `timezone`, `full_day`, `omit_timestamp`). The stacked-flag
  pre-pass turns `-yy`/`-qqq` into `-y -y`/`-q -q -q`, and the
  bespoke parser counts the repetitions natively. Composition happens
  in `datum-commands/src/cmd.rs` (`build_time_with_precision`).
- **Target (step 2/N):** `datum_parse::input::time_args::TimeArgs`
  with a `build(now_fn: impl Fn() -> DatumTime) -> (DatumTime,
  Uncertainty, TimeFromArgs)` method. The `now_fn` injection is what
  makes it WASM-safe (the host supplies "now" + local timezone).

### `durationArgs.ts` (28 LOC)

Positional `duration` + `-m/--moment` flag. `-m` deletes the
positional and sets `dur = null`.

- **Daydumb today:** `LogArgs::duration: Option<String>` and
  `moment: bool`. Duration parsing uses
  `datum_core::time::parse_duration` (now in
  `datum_parse::time::parse_duration_str`).
- **Target (step 2/N):** `datum_parse::input::duration_args::DurationArgs`
  with a `build() -> DurationIntent { None | Some(Duration) }`.

### `fieldArgs.ts` (24 LOC)

Positional `field` + `-F/--fieldless` (suppresses the positional).

- **Daydumb today:** `LogArgs::field: Option<String>` and a `fieldless`
  bool per command. The swap pre-pass makes `<field> <verb>` work.
- **Target (step 2/N):** `datum_parse::input::field_args::FieldArgs`.

### `outputArgs.ts` (37 LOC)

`--show-all/-A`, `--show`, `--format-string/-o`. Choices enum
`Show = default | none | minimal | standard | format | all`.

- **Daydumb today:** `OutputFlags` in `datum-commands/src/args.rs`.
  `--show-all` overrides, `--format-string` implies `Show::Format`
  unless another `--show` is set. Works.
- **Target (step 2/N):** `datum_parse::input::output_args::OutputArgs`
  (lift current `OutputFlags::resolve`). No semantic change.

### `quickIdArg.ts` (26 LOC)

Optional positional `quickId` + `--on-ambiguous
= all|first|last|fail|ask` (default `fail`).

- **Daydumb today:** `GetArgs::quick_id: Option<String>` etc; ambiguity
  handling is per-caller rather than a unified flag.
- **Target (step 2/N):** `datum_parse::input::quick_id_arg::QuickIdArg`
  with `on_ambiguous: OnAmbiguous`. Unify handlers so ambiguity is
  resolved once centrally.

### `dbArgs.ts` (57 LOC)

`--db`, `--host`, `--adapter`, `--username`, `--password`, `--env`,
`--project-dir`, `--config`, `--create-db`.

- **Daydumb today:** only `--db` is global (resolves through the
  XDG/`datumrc.yml` registry in `datum-commands/src/config.rs`).
  `--host`/`--username`/`--password` do not apply — daydumb stores
  locally, not against CouchDB. `--env` / `--config` / `--create-db`
  map differently (daydumb auto-creates on first access and reads
  `~/.config/datum/datumrc.yml` unconditionally).
- **Target (step 2/N):** `datum_parse::input::main_args::MainArgs`
  carries just `db: Option<String>`. The CouchDB-specific flags are
  intentionally dropped as **non-ports** (see Non-ports below).

### `topLevelHelp.ts` (53 LOC)

Raw-formatted description string for `datum --help` / `datum help`.

- **Daydumb today (post step-3/N):** `datum_parse::help` renders a
  hand-authored top-level description matching datum's layout plus
  daydumb-added commands (`spec`, `alias`, `track`, `ignore`, `info`,
  `config`, `history`, `undo`, `why`, `conflicts`, `setup`).

### `defaults.ts` (1 LOC — empty)

Placeholder. Skip.

---

## `time/` coverage — per file

### `parseTimeStr.ts` (85 LOC)

Handles: quick-style repeats (`qqqt`), HH[:MM[:SS[.ms]]] with
optional `am`/`pm` meridian, `+N`/`-N` relative durations (delegates
to `parseDurationStr`), ISO-8601 parse, and a chrono-node natural-
language fallback.

- **Daydumb today:** `parse_time_arg` in
  `datum_parse::time::parse_time_str`. Covers: "now", `+N`/`-N`
  minutes, 4-digit HHMM, absolute HH:MM[:SS], am/pm meridian, full
  ISO-8601 datetime (`2024-03-16T15:30:00Z`, `±HH:MM` offsets,
  fractional seconds, no-tz local form — shipped in da-nnr), plus
  `±N[unit]` uncertainty suffix (daydumb extension from D2).
- **Missing in daydumb:**
  - Repeated-letter quick syntax (`qqqt` == `-3q -t`).
  - Chrono-node natural-language fallback ("yesterday at noon",
    "next Thursday 3pm").
- **Added in daydumb:**
  - `±N[unit]` uncertainty suffix (not in datum).

### `parseDateStr.ts` (47 LOC)

Handles: `[+-]N` relative day offsets, ISO date, chrono-node
natural-language fallback.

- **Daydumb today:** `parse_date_arg` in
  `datum_parse::time::parse_date_str`. Covers relative integer
  offsets and `%Y-%m-%d` ISO. **Missing:** chrono-node fallback.
- **Added in daydumb:** `±N[unit]` on the date itself.

### `parseDurationStr.ts` (89 LOC)

Handles: `[+-]` sign prefix, ISO-8601 duration (`PT5M`), colon-split
(`[[[[years:]days:]hours:]minutes:]seconds`), `parse-duration`
package for named units (`5min`, `2h`, `1d`, `5ms`, ...).

- **Daydumb today:** `parse_duration` in
  `datum_parse::time::parse_duration_str`. Covers ISO-8601 (partial
  — `P` with day/month/year, `PT` with H/M/S), colon-split up to
  4 parts, and named-unit suffixes (`s/sec`, `m/min`, `h/hr`,
  `d/day`).
- **Missing in daydumb:**
  - `ms` unit (milliseconds).
  - `w`/`week` unit.
  - `y`/`year` unit (ISO only).
  - Sign prefix handling (daydumb flows sign through the numeric
    parse, which works for named-unit form but not ISO or
    colon-split).
- **Added in daydumb:** none.

### `datumTime.ts` (86 LOC)

Defines `DatumTime = { utc, o?, tz? }`; `toDatumTime(Luxon)` and
`datumTimeToLuxon` round-trips.

- **Daydumb today:** `DatumTime { utc: DateTime<Utc>,
  offset_minutes: i32, timezone: String }`.
- **Divergences:**
  - Daydumb stores `offset_minutes` instead of `o` (datum stored an
    hour-based offset). The serialized form on disk is different
    (`offset_minutes` vs `o`), which is intentional — we don't
    round-trip datum's on-disk form.
  - Daydumb's `timezone` field is required; datum's `tz` is
    optional.
  - Daydumb has no `onlyDate` representation — a date-only entry
    goes through an explicit `EntryShape::Day`.

### `getOccurTime.ts` (17 LOC)

Reads `data.occurTime` off a datum document and converts to Luxon.

- **Daydumb today:** N/A — `occurTime` is the `start_time` column
  directly on the Entry row. The `get_occur_time.rs` stub is redundant
  with `TimeArgs::build()` and slated for removal (see follow-up bead
  for stub cleanup).

### `getTimezone.ts` (25 LOC)

Resolves a timezone from number (UTC offset, hours), `"system"`,
`"UTC±N"`, or IANA name. Returns a luxon `Zone`.

- **Daydumb today:** `resolve_timezone` in
  `datum_parse::input::time_args` handles IANA names, numeric UTC
  offset (`-z -7`), `UTC±N` literals (commit 95f3824, da-xv0), and
  the `"system"` literal which resolves to the OS local zone via
  `iana_time_zone::get_timezone()` (case-insensitive — `system`,
  `SYSTEM`, `System` all work; native builds only, da-nnr).
- The `get_timezone.rs` stub in `datum-parse/src/time/` is now a
  misleading leftover (real impl lives in `input/time_args.rs`);
  flagged for cleanup in a follow-up.

### `humanTime.ts` (47 LOC)

Renders a DatumTime for display: relative day prefix (`-2d`, `+1d`,
ISO date for further), local HH:MM:SS, gray offset, chalk
underline on future.

- **Daydumb today:** the relative-day prefix lives in
  `datum_commands::render::action::format_relative_day_prefix`
  (single source of truth, da-n1q). The unified `format_timestamp`
  (da-d5w.1) is the sole timestamp renderer — it is called by both
  the info-line path and the table extractor (via the
  `synthesize_local_datumtime` bridge for `create_time` / `modify_time`).
  `datum tail`, `datum now`'s last-seen column, and the write-action
  info line all share the same `-2d` / `+1d` / ISO-date logic.
  Rendering is in the row's own timezone (so "today" follows the row's
  local calendar). Future-time underline ships in the new renderer
  (da-vkj). The `human_time.rs` stub in `datum-parse/src/time/` is a
  misleading leftover — flagged for cleanup in the same follow-up bead.

### `timeUtils.ts` (40 LOC)

`now(zone?)`, `today(zone?)`, `defaultZone`, `isIsoDateOrTime`,
`isoDateFromDateTime`, `isoDatetimeFromDateTime`,
`isoDurationFromDuration`, `utcOffset`.

- **Daydumb today:** spread across `datum_parse::time::datum_time`
  (`now()` behind the `native` feature) and chrono's built-ins.
  **No explicit `is_iso_date_or_time` helper** — daydumb treats
  invalid ISO as a parser error rather than a branch.
- **Target (step 2/N):** expose the helpers that are actually used
  (`iso_duration_from_duration`) as a small utils module.

---

## Flexibility audit

| Feature | datum | daydumb (target, post-R2) | daydumb (today) |
|---|---|---|---|
| `flexiblePositional` | ✓ | ✓ (step 2/N) | partial (per-command ad-hoc) |
| `key=value` interleaved with positional | ✓ everywhere | ✓ (step 2/N) | partial (log handler only) |
| `-k key` (required) | ✓ | ✓ (step 2/N) | ✓ |
| `-k key=` (optional, no default) | ✓ | ✓ (step 2/N) | ✓ |
| `-k key=default` | ✓ | ✓ (step 2/N) | ✓ |
| `-K key` (id-slug key) | ✓ | ✓ (step 2/N) | partial |
| `-R`/`-S`/`-C`/`-l`/`--strict` remainder | ✓ | ✓ (step 2/N) | missing |
| `.` skip token | ✓ | ✓ (step 2/N) | missing |
| Command-change escape mid-parse | ✓ | N/A (intentional) | intentional non-port |
| `-d`/`-t`/`-y*`/`-q*` stackable | ✓ (`-q*` via count) | ✓ | ✓ |
| `±N[unit]` uncertainty (D2) | ✗ | ✓ | ✓ (already in `datum-parse`) |
| Field/verb swap (`<field> <verb>`) | ✗ | ✓ | ✓ |
| Terminal-verb override (`<field> end`) | ✗ | ✓ | ✓ |
| DB-backed alias pre-pass | ✗ | ✓ (in `datum_parse::alias`) | ✓ (in `datum_parse::alias`, post step-3/N) |
| Spec-driven auto-alias | ✗ | ✓ (planned D3) | stubbed |
| String API (`datum_parse::parse_argv_string(&str)`) | ✓ (via `shell-quote`) | ✓ (step 3/N) | ✓ (post step-3/N) |
| Typed API (construct `CommandIntent` directly) | ✗ | ✓ (step 2/N) | ✓ (post step-3/N — `datum-parse::Command` enum) |
| WASM-compilable parse layer | ✗ | ✓ | ✓ for `datum-parse` as of this commit |

---

## Tests in datum not yet in daydumb

TS test files under `/tmp/datum-survey/src/{input,time}/__test__/` —
mechanically unported as of R2 step 1/N. Port responsibility per
submodule:

| TS file | LOC | Port to (Rust) | Owner step |
|---|---|---|---|
| `input/__test__/consolidateKeys.test.ts` | 21 | `datum-parse/src/input/consolidate_keys.rs` tests | step 2/N |
| `input/__test__/dataArgs.test.ts` | 765 | `datum-parse/src/input/data_args.rs` tests + `crates/datum-cli/tests/data_args_parity.rs` | step 2/N |
| `input/__test__/flexiblePositional.test.ts` | 71 | `datum-parse/src/input/flexible_positional.rs` tests | step 2/N |
| `input/__test__/mainArgs.test.ts` | 202 | `datum-parse/tests/parse_argv.rs` | step 3/N |
| `input/__test__/timeArgs.test.ts` | 394 | `datum-parse/src/input/time_args.rs` tests | step 2/N |
| `time/__test__/parseTimeStr.test.ts` | 186 | `datum-parse/src/time/parse_time_str.rs` tests (expand existing) | step 2/N |
| `time/__test__/parseDateStr.test.ts` | 102 | `datum-parse/src/time/parse_date_str.rs` tests | step 2/N |
| `time/__test__/parseDurationStr.test.ts` | 53 | `datum-parse/src/time/parse_duration_str.rs` tests | step 2/N |
| `time/__test__/datumTime.test.ts` | 34 | `datum-parse/src/time/datum_time.rs` tests | step 2/N |
| `time/__test__/getOccurTime.test.ts` | 84 | N/A — redundant with `TimeArgs::build()`; not ported | — |
| `time/__test__/humanTime.test.ts` | 93 | `datum-commands/src/render/` tests (canonical impl lives there) | step 2/N |

Tests already ported at R2 step 1/N (retained from the original
`datum-core::time`): the 14 tests in `datum-parse/src/time/tests.rs`
covering stacked-flag counting, time/date parsing, duration parsing
(colon / named unit / ISO), `±N[unit]` uncertainty, and the `-q*`
precision scaling rule.

---

## Non-ports (intentional)

Deliberately *not* carried forward from datum:

- **CouchDB-oriented `dbArgs`** (`--host`, `--adapter`, `--username`,
  `--password`, `--env`, `--create-db`, `--project-dir`). Daydumb is
  SQLite-local with a `datumrc.yml` registry; these flags have no
  sensible analogue.
- **`chrono-node` natural-language parse** (in `parseTimeStr.ts` /
  `parseDateStr.ts`). Keeps `datum-parse` free of a heavy English-
  only NLP dep, which would also blow up WASM bundle size. Daydumb
  callers who want NL parsing can preprocess before calling
  `datum_parse::parse_argv`.
- **Command-change mid-parse escape** (dataArgs.ts §8). Daydumb
  dispatches verb-first; switching verbs during data-arg parsing was
  a datum ergonomic hack that conflicted with the field/verb swap
  pre-pass. Users who want a different verb should write a different
  verb.
- **`v1Cmd`** (datum's legacy-format exporter). Migration goes the
  other direction (datum JSON → daydumb SQLite via `datum db import`);
  exporting back out is not a goal.

---

## Improvement ideas (post-R2)

- **String-API fuzz harness.** Once `datum_parse::parse_argv_string`
  lands (step 3/N), add a quickcheck-style harness that round-trips
  random argvs through `parse_argv → render → parse_argv` to catch
  asymmetries. Analogous would be nice in datum but the JS
  ecosystem's fuzz tooling is weaker.
- **Help output diffing.** `datum_parse::help::print_help()` should
  render identically to `datum --help` on the shared subset of
  commands. A golden-file test in `datum-parse/tests/help_parity.rs`
  would keep the two in sync as commands evolve.
- **Auto-alias generator from field spec** (planned D3). The parity
  note there is: datum has no equivalent — this is a daydumb-native
  feature that piggybacks on the new alias infrastructure. Ensure
  the generated aliases are addressable via both `datum_parse::alias`
  and the DB.
- **`onAmbiguous` unification.** Right now daydumb handlers choose
  their own ambiguity behaviour. Centralising on a single
  `OnAmbiguous` enum (per `quickIdArg.ts`) simplifies `get`, `del`,
  `edit`, `retime`, `update`, and the new D5 conflict-queue path.
- **`-b/--base-data` JSON seed.** Low user demand but cheap to add
  once `data_args` owns merge semantics — effectively a
  `starts_with_base: Option<serde_json::Value>` on the `DataArgs`
  struct.
- **Quick-letter time syntax (`qqqt`).** datum's most delightful
  flourish. Worth porting once `parseTimeStr` has a proper scanner
  rather than a cascade of `if let Ok`.

---

## Acceptance-criteria checklist (bead `da-t6t.2`)

| Criterion | Status |
|---|---|
| `cargo build --workspace` succeeds | ✓ |
| `cargo test --workspace` passes (no regressions) | ✓ (431 tests green) |
| `cargo build --target wasm32-unknown-unknown -p datum-parse` succeeds | ✓ (uses `--no-default-features`) |
| `datum-cli/Cargo.toml` no longer lists `clap` | ✓ (moved behind `datum_commands::cmd::dispatch_argv`) |
| `datum-parse/` covers all ported `input/` and `time/` behaviors with tests | ✓ for what's ported (time, consolidate_keys, flexible_positional); remaining input/ submodules are stubs awaiting step 2b/N |
| `--help` output is readable and covers every command | ✓ (rendered by `datum_parse::help` post step-3/N) |
| `docs/parity-audit/parsing.md` exists and follows spec | ✓ |

**Net clap footprint after R2:** **zero.** Step 3/N (commit 74fac61,
da-jzj) removed clap from the entire workspace; `datum-parse` owns
argv parsing and `--help` rendering for all commands.
