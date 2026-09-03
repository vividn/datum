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

This produces a `datum` binary on your `PATH`. CLI startup is a hard
constraint: under 5 ms cold, on the order of microseconds warm.

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
  following_state) by `derive_verb` (datum-core::verb): `point`,
  `switch`, `block`, `block-switch`, `point-switch`. The CLI verbs
  `add`/`occur`/`start`/`stop`/`switch` map onto these via the truth
  table in `crates/datum-core/src/verb.rs`.
- **State** — derived from state-bearing entries (`Switch`,
  `BlockSwitch`, `PointSwitch`, `Block` with state=set, `Point` as a
  false-event marker): at any past instant we can reconstruct a
  field's value. `datum now` consults this.
- **Slug vs. hid** — `slug` is the human-readable id (`mood:2026-04-29T09:15Z`).
  `hid` is the short hash you see in `datum tail`; both resolve to the
  same entry. `datum tail` also prints an `_n` column: `_1` is the most
  recent entry, `_2` the one before it, and so on, so `datum del _3`
  names the row you just read. Ordinals count every live entry, so a
  field-filtered listing shows gaps — `_3` always means the same row it
  does everywhere else.
- **Intent** — the original argv is captured and replayed under a
  pinned `parser_version`, so a future parser fix can retroactively
  correct old entries without losing what the user actually typed.

## Commands

The list below is generated from `datum_parse::help::render_markdown(None)`.
**Do not edit by hand** — `cargo xtask readme` regenerates it, and the
`readme_commands_section_in_sync` test in `datum-cli` fails CI on drift.

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

- `edit` — Open an entry in $EDITOR
- `update` — Replace keys on an existing entry
- `merge` — Deep-merge new values into an entry
- `rekey` — Copy an entry to a new id (old row stays)
- `retime` — Shift time_start/time_end on entries (alias: rt)
- `delete` — Delete an entry (alias: del)
- `undo` — Undo the last change to an entry (revert via history)

## READ

- `tail` — Show recent entries
- `head` — Show oldest entries
- `get` — Print a single entry by id or slug
- `grep` — Regex search over entry JSON content
- `history` — Show the full modification trail for an entry
- `why` — Explain the intent behind an entry

## RENDER

- `dayview` — Render a dayview SVG (or dump layout JSON)
- `nowview` — Render a nowview SVG (drop -o to write to stdout)
- `now` — Show current state of tracked fields + terminal nowview band

## ADMIN

- `spec` — Manage field specs (schemas)
- `alias` — Manage aliases
- `check` — Run database consistency checks
- `db` — DB-lifecycle (setup, purge, backup, restore, migrate, import)
- `info` — Show health/metadata for DB(s)
- `config` — Show or manage user configuration
- `meta` — Attach or list protocol-change notes on a field
- `kit` — Install / export / list field-spec kits (TOML bundles)
- `reparse` — Replay an entry's stored raw_input through the current parser; --fix rewrites drifted columns

## VIEWS / SYNC

- `count` — Count entries (optionally filtered/grouped/since-bounded)
- `stats` — Count, total duration, and per-state ratio over a time slice
- `ids` — List entry ids (newest-first by default)
- `sync` — Push/pull changesets, manage auth & keys
- `conflicts` — Inspect and resolve sync conflicts
- `view` — Execute a named SELECT view and render the rows

## Global Options

- `--db <NAME_OR_PATH>` — Override the active DB
- `--show <MODE>` — default|none|minimal|standard|format|all
- `-A, --show-all` — Shorthand for --show all
- `-o, --format-string` — %field%-templated output string
- `-h, --help` — Show this help
- `-V, --version` — Show version
<!-- END AUTOGEN: COMMANDS -->

## Atomic multi-entry aliases

A standalone, whitespace-bounded `,` token splits one invocation into
N sides that all write atomically with the same `occur_time`. The
trigger works at the top-level CLI, inside alias bodies, and through
chains of alias-of-alias (multi-of-multi flattens to N flat sides).

### Comma trigger

The comma must be its own token — whitespace on both sides:

```bash,no_run
datum vertical , sit     # multi (whitespace-bounded)
datum vertical, sit      # NOT a multi — `vertical,` is one token
datum vertical ,sit      # NOT a multi — `,sit` is one token
datum vertical,sit       # NOT a multi — single token
datum occur k=a,b        # commas inside values pass through unchanged
```

There is no escape syntax; whitespace is the trigger. If you want a
literal `,` in a value, embed it (`key=a,b`) and the parser leaves it alone.

### Example 1: bare comma at the CLI

```bash,no_run
datum switch body vertical , switch body sit
```

Both sides share one `occur_time`. Because they target the same field
(`body`), they collapse into a single row with
`state=Parallel([vertical, sit])` — the surprise to know about: two
switches to the same field at the same instant don't overwrite each
other, they merge into a parallel state. Different-field sides write
separate rows (still atomic).

### Example 2: defining a multi-entry alias

```bash,no_run
datum alias set oov   "switch environment outside , switch body vertical"
datum alias set scaff "switch body sit , occur caffeine -k doseMg="
datum oov                # writes both rows atomically
datum scaff 100          # positional fills caffeine's doseMg slot
```

Positional args fill alias slots left-to-right across sides. The
trichotomy of `-k` (and `-K`) declaration forms governs each slot:

| Form         | Meaning                                                |
|--------------|--------------------------------------------------------|
| `-k key`     | required — errors at call time if no value is supplied |
| `-k key=`    | optional, no default — silently skipped if unfilled    |
| `-k key=val` | defaulted — auto-filled with `val` when unfilled       |

A `-k` (or `-K`) name declared on multiple sides becomes a *shared* slot
— one call-site value propagates to every declaring side. The `.` token
skips a slot on its side; `.` for a shared slot skips on all sides.

### Example 3: call-site time flags compose with alias-bound time

```bash,no_run
datum alias set vvcaf "switch body vertical , occur caffeine -t +5"
datum vvcaf -t +10       # vertical at +10min, caffeine at +15min
```

Call-site time flags *compose* with alias-bound times (they do not
override). Inside an alias body, flags before a `,` scope to their
side. `--` is implicit at the end of every alias body, so call-site
flags apply to all sides by default. A `--` inside an alias body
behaves the same way locally but its scope ends at the alias boundary.

### `_LAST` after a multi-write

After a multi-entry write, `_LAST` refers to **every** entry written by
that invocation. Follow-up commands like `rekey _LAST oldkey=newkey`
therefore affect every row of the atomic group. A subsequent
single-entry write resets `_LAST` to that one row.

For the condensed in-app reference, run `datum alias --help`; the full
behavioural contract is in [`docs/alias-system-design.md`](docs/alias-system-design.md).

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

Notable env vars:

| Variable                | Purpose                                                |
|-------------------------|--------------------------------------------------------|
| `DATUM_DB`              | Path or registry name of the active database.          |
| `DATUM_HOME`            | Override the data directory (default: OS data dir).    |
| `DATUM_CONFIG`          | Path to a specific `datumrc.toml`.                      |
| `DATUM_CONFIG_HOME`     | Override the directory `datumrc.toml` lives in.         |
| `DATUM_DEFAULT_FIELD`   | Field used when one is omitted from the argv.          |
| `DATUM_DEVICE_ID`       | Override the per-device id stamped on entries.         |
| `DATUM_EDITOR`          | Editor for `datum edit` (falls back to `EDITOR`).      |
| `DATUM_COLOR`           | `0` disables colour; otherwise auto-detect from TTY.   |
| `DATUM_WEEK_START`      | Weekday a week opens on (`monday`…`sunday`, or `1`-`7`). |

`week_start` (env `DATUM_WEEK_START`, toml `week_start = "monday"`) says
which weekday opens a week for `datum stats --week` / `--last-week`. A
weekday name is canonical; an ISO number (`1` = Monday … `7` = Sunday) is
also accepted. The default is Monday.

The default database lives at `$DATUM_HOME/default.db` (creating the
directory if needed). The active DB can also be overridden per-invocation
with `--db <name-or-path>`. Named databases are tracked in
`$DATUM_CONFIG_HOME/datum/dbs.toml`.

`--show` controls how much of each entry is printed after a logging
command: `none`, `minimal`, `standard`, `format` (template-driven), or
`all`. `-A` is a shorthand for `--show all`.

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
