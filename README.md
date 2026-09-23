# datum

A personal data tracker. Inspired by [the original
datum](https://github.com/vividn/datum) — a 10-year personal project
that produced this rewrite — `datum` lets you log moments and states from
your own life and stitch them back together as timelines and statistics.
The act of logging forces a small moment of reflection; the visualisation
turns the result into something worth looking at.

`datum` is **local-first**: every entry is written to a SQLite database
on your machine, and the CLI never makes network calls in the hot path.
An optional encrypted relay (see `crates/datum-sync-server/`) shuttles
data between your devices end-to-end-encrypted; the relay is content-blind.

## Status

**Alpha.** Data model v2 (locked in 2026-04-21); breaking changes to
storage layout, command surface, and CLI grammar are still in scope.
Use it on a database you can afford to throw away.

## Install

There are no published binaries yet. From a checkout:

```bash,no_run
cargo install --path crates/datum-cli --force
```

Or run the equivalent helper, which is also the one-command upgrade:

```bash,no_run
scripts/install.sh
```

This produces a `datum` binary on your `PATH`.

The `--force` flag is required: `datum` is pre-release and not on
crates.io, so the workspace version stays fixed across builds. Without
`--force`, `cargo install` sees the same version already installed and
silently does nothing — your binary would never update.

## Quickstart

```bash
# Track the current state of a field
datum switch mood happy

# Log a point-in-time event
datum occur coffee

# Log an event with structured data (use `switch` for stateful values;
# `occur` is reserved for stateless points-in-time)
datum switch weight 75.2kg

# Time something — start a block now, stop it later
datum start work project=daydumb
datum stop work

# Look back
datum tail            # most recent 10 entries
datum tail mood       # filtered to one field
datum now             # current state of all tracked fields
```

Every flag and every verb is documented in `datum --help`,
`datum <command> --help`, and the man pages (see [Man pages](#man-pages)).
The punctuation — `,` `,,` `(` `)` `--` `±` `%…%` and the rest — is
[`docs/grammar-tokens.md`](docs/grammar-tokens.md).

## Concepts

One-line summaries of the model:

- **Entry** — one row. Has an id (ULID), a slug (`field:time`), a field,
  a verb (storage classifier), an `occur_time`, a `state` JSON value, an
  optional ISO-8601 `dur`, an optional `following_state`, and arbitrarily
  nested `extra_data`.
- **Field** — what is being tracked: `mood`, `coffee`, `weight`,
  `project`. Freeform by default; can be schema-constrained via
  `datum spec set`.
- **Action** — the user's command intent at the CLI: `add`, `occur`,
  `start`, `stop`, `switch`. Action is parsed from argv, canonicalizes
  the (state, dur, following_state) triple (`start` ⇒ state=true,
  `stop` ⇒ state=false), and is then discarded — only the triple
  reaches storage.
- **Verb** — the storage classifier derived from (state, dur,
  following_state) by `derive_verb` (datum-parse::verb): `point`,
  `switch`, `block`, `block-switch`, `point-switch`. The CLI verbs
  `add`/`occur`/`start`/`stop`/`switch` map onto these via the truth
  table in `crates/datum-parse/src/verb.rs`.
- **State** — derived from state-bearing entries (`Switch`,
  `BlockSwitch`, `PointSwitch`, `Block` with state=set, `Point` as a
  false-event marker): at any past instant we can reconstruct a
  field's value. `datum now` consults this.
- **Tracked** — a field's standing claim about its own GAPS. A tracked
  field is one you are actively recording, so a stretch of it with no
  entries is not missing data, it is evidence: it says the events did
  not happen. `datum ignore` withdraws that claim from there on — over
  an ignored stretch a gap says only that you stopped writing things
  down, and says nothing about whether events occurred. That is the
  whole reason `ignore` exists: without it a reader cannot tell "I did
  not do this" from "I was not logging this", and those are different
  facts about a life. Tracked is the default — every field that has
  been logged or declared is tracked until an ignore says otherwise.
- **TRACKED and IGNORED** — the two special `state` values. They are
  states a FIELD is in, carried by ordinary rows in the same `state`
  column every other value lands in — not flags on a row, not columns
  of their own. A field is ignored when its newest state-bearing row is
  `IGNORED`, and the next `occur`/`start`/`switch`/`track` on it tracks
  it again; so tracked-ness is read off the entry log
  (`is_ignored_state` in `crates/datum-parse/src/shape.rs`,
  `Db::get_tracked_fields` in `crates/datum-core/src/db/state.rs`),
  which is what `datum fields` filters on, what `datum now` hides
  (`now -A` shows them again), and what `datum info` tallies as
  `tracked`/`ignored`. `TRACKED` is a different thing wearing the same
  word: the OFF state `datum track` writes on an `occur`/`add` field,
  "declared, nothing logged yet", which reads as `false` everywhere but
  the printed text. It is not what makes a field tracked, and no row
  carrying it is ever required.
- **Slug vs. hid** — `slug` is the human-readable id (`mood:2026-04-29T09:15Z`).
  `hid` is the short hash you see in `datum tail`; both resolve to the
  same entry. `datum tail` also prints an `_n` column: `_1` is the most
  recent entry, `_2` the one before it, and so on, so `datum del _3`
  names the row you just read. Ordinals count every live entry, so a
  field-filtered listing shows gaps — `_3` always means the same row it
  does everywhere else. Two ordinals joined by a dash name the whole run
  between them, so `datum del _2-_10` acts on nine rows in one go
  (`_10-_2` is the same run, `mood:_1-_3` counts inside one field, and a
  range running off the end acts on the rows that exist). The command
  prints one action record per entry it touched, so you see exactly what
  the range caught.
- **Intent** — the original argv is captured and replayed under a
  pinned `parser_version`, so a future parser fix can retroactively
  correct old entries without losing what the user actually typed.

The command `datum track` and the tracked-ness above are not the same
thing, and it is worth keeping them apart. The COMMAND declares a field:
it writes the spec that gives the field its verb, its colour and its
states, and one row saying the declaration happened (`datum help
track`). The STATE is the default every logged field already has. A
field you never tracked is tracked; a field you tracked and then
ignored is not. Tracking is a thing you say about a field, ignoring is
a thing you say about a stretch of time.

## Commands

The list below is generated from `datum_parse::help::render_markdown(None)`.
**Do not edit by hand** — `cargo xtask readme` regenerates every
`<!-- BEGIN AUTOGEN: … -->` region in this file, and the
`readme_autogen_sections_in_sync` test in `datum-cli` fails CI on drift.
Each command's sentence here is the same one its own `--help` page is
titled with; there is one field behind both.

<!-- BEGIN AUTOGEN: COMMANDS -->
## Usage

```
datum [--db NAME_OR_PATH] <command> [args]
```

## LOGGING

- `add` — Register a keyed timeless record (needs an id)
- `note` — Jot a timeless entry (auto create_time id)
- `occur` — Record a point-in-time event
- `start` — Begin a duration-tracked activity
- `stop` — End a duration-tracked activity (alias: end)
- `switch` — Transition a field to a new state
- `track` — Track a field in the now view
- `ignore` — Ignore a field in the now view
- `flag` — Mark a field for review (lightweight 'come back to this')

## MUTATE

- `edit` — Open an entry in $DATUM_EDITOR (or $EDITOR)
- `update` — Replace keys on an existing entry
- `merge` — Deep-merge new values into an entry
- `rekey` — Copy an entry to a new id (old row stays)
- `retime` — Move an entry's occur_time (alias: rt)
- `delete` — Delete an entry (alias: del)
- `undo` — Undo the last change to an entry (revert via history)
- `redo` — Redo the last undone change (the inverse of undo)

## READ

- `tail` — Show recent entries
- `head` — Show oldest entries
- `get` — Print entries by id or slug
- `grep` — Regex search over entry JSON content
- `history` — Show the full modification trail for an entry
- `why` — Explain the intent behind an entry

## RENDER

- `dayview` — Render a multi-day timeline as SVG/PNG/HTML
- `nowview` — Paint each field's state now as a terminal band (-o writes a file)
- `now` — Show the current state of tracked fields + terminal nowview band

## ADMIN

- `spec` — Manage field specs (schemas)
- `alias` — Manage aliases
- `check` — Run database consistency checks
- `db` — DB-lifecycle (setup, purge, backup, restore, migrate, v2import)
- `info` — Show health/metadata for DB(s)
- `config` — Show or manage user configuration
- `meta` — Attach or list protocol-change notes on a field
- `kit` — Install / export / list field-spec kits (TOML bundles)
- `reparse` — Replay an entry's stored raw_input through the current parser

## VIEWS / SYNC

- `count` — Count entries (optionally filtered/grouped/since-bounded)
- `stats` — Count, total duration, and per-state ratio over a time slice
- `ids` — List entry ids (newest-first by default)
- `fields` — List every field: its spec and the states it was logged with
- `sync` — Push/pull changesets, manage auth & keys
- `conflicts` — Triage sync conflicts (see `datum check` for integrity findings)
- `view` — Execute a named SELECT view and render the rows

## Global Options

- `--db <NAME_OR_PATH>` — Override the active DB
- `--show <MODE>` — default|none|minimal|standard|format|all|json
- `--json` — Shorthand for --show json
- `-A, --show-all` — Shorthand for --show all
- `-o, --format-string` — %field%-templated output string
- `-h, --help` — Show this help
- `-V, --version` — Show version
<!-- END AUTOGEN: COMMANDS -->

## Aliases

The reference below is **not written here**. It is generated by
`cargo xtask readme` from the table that answers `datum help alias`, so
the two cannot disagree — this section used to be a hand copy of that
page, and the copy and the original had reached the same wrong claim from
opposite directions. Run `datum help alias` for the same text without a
checkout; the full behavioural contract is in
[`docs/alias-system-design.md`](docs/alias-system-design.md). The tokens
`,,`, `(`, `)` and `\(` that alias expansion emits — and the rest of the
grammar's punctuation — are in
[`docs/grammar-tokens.md`](docs/grammar-tokens.md).

<!-- BEGIN AUTOGEN: ALIASES -->
### `datum alias`

Manage aliases

Aliases expand argv before the parser sees it, so an alias can stand in for any prefix of a `datum` invocation — verb, field, flags, or all three.

A standalone, whitespace-bounded `,` token splits one invocation into N sides that write atomically with the same occur_time. The trigger works at the top-level CLI, inside alias bodies, and through chains of alias-of-alias (multi-of-multi flattens to N flat sides). The comma must be its own token: `vertical , sit` triggers a multi, while `vertical,sit`, `vertical ,sit`, and `vertical, sit` do not. Values with embedded commas (`key=a,b`) pass through unchanged.

Key slot forms (`-k` and `-K` accept the same three shapes): `-k key` is required and errors at call time if unfilled, `-k key=` is optional with no default (silently skipped), `-k key=val` defaults to `val` when no value is supplied. A `-k` or `-K` name declared on multiple sides becomes a shared slot — one call-site value propagates to every declaring side. The `.` token skips a slot on its side; for a shared slot it skips on all sides.

`--` semantics in multi-entry contexts: inside an alias body, flags before `--` scope to their side and flags after `--` apply to all sides. `--` is implicit at the end of every alias body, so call-site flags apply to all sides by default. A `--` inside an alias body does not propagate past the alias boundary.

Time flags at the call site: the stackable offsets (`-y`, `-q`, `-d`) shift whatever time the alias bound, but a call-site `-t` currently REPLACES an alias-bound `-t` rather than adding to it — on an alias bound to `-t +5`, `-t +10` writes at +10, not +15. That last part is a known divergence from the alias spec's Rule 15 and is under review (da-snlq1), so do not read it as settled. In a multi-entry alias every side writes at ONE occur_time, taken from the first side, so a later side's `-t` is dropped.

After a multi-entry write, `_LAST` refers to every entry written by the most recent invocation, so `rekey _LAST oldkey=newkey` affects every row of the atomic group. A subsequent single-entry write resets `_LAST` to that one row.

#### Usage

```
datum alias show <NAME>
datum alias set <NAME> <EXPANSION>
datum alias list
datum alias remove <NAME>
```

#### Output

- `--show <MODE>` — default|none|json
- `--json` — Shorthand for --show json

#### Examples

```
datum alias set vbp "note word srcLang=pt pos=verb"
datum vbp correr "to run"
```

An alias stands in for any PREFIX of an invocation — verb, field, flags, data keys. The call above writes a `word` note with `srcLang=pt`, `pos=verb` and `comment="correr to run"`.

```
datum switch body vertical , switch body sit
```

A whitespace-bounded `,` splits one invocation into two sides that write atomically at one `occur_time`. Same field, same instant, so the two merge into ONE `body` row with `state = [vertical, sit]` rather than overwriting each other.

```
datum alias set oov "switch environment outside , switch body vertical"
datum oov
```

The same trigger inside an alias body: `datum oov` writes both rows atomically.

```
datum alias set caf5 "occur caffeine -t +5"
datum caf5        # now + 5
datum caf5 -t +10 # now + 10, NOT now + 15
datum caf5 -q     # now + 5, then 5 back: now
```

`-y` / `-q` / `-d` shift whatever time the alias bound. A call-site `-t` currently REPLACES an alias-bound `-t` instead of adding to it — a known divergence from the alias spec's Rule 15, under review (da-snlq1), not a settled contract.

<!-- END AUTOGEN: ALIASES -->

## Sync

Sync is opt-in and end-to-end encrypted. The relay server stores opaque
ciphertext blobs and the metadata (device id, sequence number) it needs
to fan changesets out to your other devices. It cannot read your data.

- `crates/datum-sync-client/` — wire protocol, crypto, HTTP client.
- `crates/datum-sync-server/` — Rust + axum relay.
- `crates/datum-sync-agent/` — background daemon (planned).

Per-crate READMEs are planned.

## Configuration

`datum` is configured through (in order of precedence):

1. CLI flags (`--db`, `--show`, ...).
2. Environment variables.
3. `datumrc.toml` in `$DATUM_CONFIG_HOME` (or the OS default config dir).

The environment variables `datum` reads, generated by `cargo xtask readme`
from the table the binary itself carries:

<!-- BEGIN AUTOGEN: ENV -->
| Variable              | Purpose                                                                           |
|-----------------------|-----------------------------------------------------------------------------------|
| `DATUM_DB`            | Path or registry name of the active database (`--db` wins).                       |
| `DATUM_HOME`          | Override the data directory (default: the OS data dir).                           |
| `DATUM_CONFIG`        | Path to a specific `datumrc.toml`.                                                |
| `DATUM_CONFIG_HOME`   | Override the directory `datumrc.toml` lives in.                                   |
| `DATUM_DEFAULT_FIELD` | Field used when one is omitted from the argv.                                     |
| `DATUM_DEVICE_ID`     | Override the per-device id stamped on entries.                                    |
| `DATUM_EDITOR`        | Editor for `datum edit`. Read FIRST, before `$EDITOR`.                            |
| `DATUM_ID_DELIMITER`  | Delimiter that collapses empty `%key%` segments in a slug template (default `:`). |
| `DATUM_WEEK_START`    | Weekday a week opens on (`monday`…`sunday`, or `1`-`7`; default Monday).          |
| `DATUM_COLOR`         | `0` disables colour; otherwise auto-detect from the TTY.                          |
<!-- END AUTOGEN: ENV -->

`week_start` (env `DATUM_WEEK_START`, toml `week_start = "monday"`) says
which weekday opens a week for `datum stats --week` / `--last-week`. A
weekday name is canonical; an ISO number (`1` = Monday … `7` = Sunday) is
also accepted. The default is Monday.

The default database lives at `$DATUM_HOME/default.db` (creating the
directory if needed). The active DB can also be overridden per-invocation
with `--db <name-or-path>`. Named databases are tracked in
`$DATUM_CONFIG_HOME/datum/dbs.toml`.

`--show` controls how much of each entry a command prints. Its modes are
listed under [Global Options](#global-options) above — generated, so they
stay the set the binary accepts — and `datum <command> --help` names the
subset that command actually draws. `-A` is shorthand for `--show all`.

## Man pages

`datum --man` prints the top-level man page to stdout in roff format;
`datum --man <subcommand>` does the same for one subcommand. Useful for
ad-hoc viewing without installing the pages:

```bash,no_run
man -l <(datum --man tail)
```

To generate the man pages on disk for distro packaging, run:

```bash,no_run
cargo xtask manpages           # writes man/datum*.1
cargo xtask manpages /tmp/m    # custom output dir
```

The roff is generated from the same `CommandSpec` table that drives
`--help`, so it never drifts from the binary it ships with.

## Pre-submit check

Before opening a PR, run the local mirror of the CI battery:

```bash,no_run
./scripts/check.sh
```

This runs `cargo fmt --check`, `cargo clippy -D warnings`,
`cargo build --workspace --all-targets`, the test suite (via
`cargo-nextest` when installed, otherwise `cargo test`), and the doc
tests — the same set the GitHub Actions workflow gates on. Run it
locally first because the repo is private (no remote branch
protection); a green script run is the contract that CI will be green.

For fast iteration on small, localised changes there is also
`scripts/check-changed.sh`: the same battery, but scoped to the
workspace crates the diff touches plus their transitive workspace
dependents (so a one-line change in `datum-cli` does not rebuild and
retest `datum-sync-*`). The script falls back to the full battery
above when a workspace-root config file changes, when nothing
crate-shaped is touched, or when the dependent closure already covers
≥75% of the workspace. Use `check-changed.sh` for fast local iteration;
CI always runs the full `check.sh` battery as the safety net.

### `cargo xtask check`

The same battery is reachable from the task runner, which additionally
runs the four repo-hygiene gates:

```bash,no_run
cargo xtask check all     # hygiene gates, then fmt, clippy, build, tests, doc tests
cargo xtask check lint    # the four hygiene gates only — compiles nothing, seconds
```

`check all` prints each step, its result, and a closing summary naming
every step that ran — read the output rather than the command name to
know what was covered. Two CI jobs are **not** covered, and `check all`
says so on every run:

- `e2e-full` — the feature-gated E2E cohort
  (`cargo nextest run -p datum-commands --test e2e --features e2e-full`)
- `wasm-check` — `cargo check -p datum-sync-client --target
  wasm32-unknown-unknown --no-default-features --features wasm`

A step that cannot run fails the battery. If `cargo-nextest` is not
installed, `check all` exits non-zero and says so; it never substitutes
another command and reports green. (`scripts/check.sh` does fall back to
`cargo test` — deliberate for a convenience script, wrong for a gate.)

The individual gates stay addressable by name for debugging a single
regression: `cargo xtask check no-cfg-any`, `no-r2-stubs`,
`no-legacy-verbs`, `max-file-size`. `scripts/check-honesty.sh` is the
hygiene-only battery as a pre-commit hook.

To run the same battery automatically on every `git push`, enable the
tracked pre-push hook once per clone:

```bash,no_run
git config core.hooksPath .githooks
```

`.githooks/pre-push` invokes `scripts/check.sh`; a non-zero exit aborts
the push. Use `git push --no-verify` as an escape hatch (CI will still
gate the merge).

If a known-noisy test trips the pre-submit gate, consult
[`FLAKES.md`](FLAKES.md) — the registry of known-flaky and
brittle-golden tests, with retry / regen guidance and an append
protocol so the catalogue grows when new noise is observed.

## Repository layout

```
crates/
  datum-core/         data model, SQLite, time parsing, field specs
  datum-cli/          the `datum` binary (clap-free; bespoke parser)
  datum-commands/     command-layer crate dispatched by datum-cli
  datum-parse/        argv parser + help-text introspection
  datum-layout/       dayview / nowview layout primitives
  datum-sync-client/  sync wire protocol, crypto, HTTP
  datum-sync-server/  encrypted relay
  datum-sync-agent/   background sync daemon (stub)
web/                  SvelteKit PWA (OPFS + SQLite WASM)
xtask/                workspace task runner (`cargo xtask <task>`)
```

## License

MIT — see `LICENSE`.
