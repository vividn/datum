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
Architectural rationale lives in the ADRs: `da-vm0` (visualisation
priorities), `da-ydg` (tech-stack decisions), `da-ylm` (data-model and
field-spec system).

## Status

**Alpha.** Data model v2 (locked in 2026-04-21); breaking changes to
storage layout, command surface, and CLI grammar are still in scope.
Use it on a database you can afford to throw away. See `da-ylm` for
the current data-model contract.

## Install

There are no published binaries yet. From a checkout:

```bash,no_run
cargo install --path crates/datum-cli
```

This produces a `datum` binary on your `PATH`. CLI startup is a hard
constraint: under 5 ms cold, on the order of microseconds warm.

## Quickstart

```bash
# Track the current state of a field
datum switch mood happy

# Log a point-in-time event
datum occur coffee

# Log an event with structured data
datum occur weight 75.2kg

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

The full conceptual model is documented in the ADRs; one-line summaries:

- **Entry** — one row. Has an id (ULID), a slug (`field:time`), a field,
  a verb, a value, optional structured `extra_data`, and a time range.
  See `da-ylm`.
- **Field** — what is being tracked: `mood`, `coffee`, `weight`,
  `project`. Freeform by default; can be schema-constrained via
  `datum spec set`.
- **Verb** — `add` (canonical universal verb), plus four sugar verbs:
  `occur` (point), `start` / `stop` (duration), `switch` (state
  transition). `block` is a fifth shape inferred from temporal keys.
- **Shape** — `Point` | `Block` | `Ongoing`. Inferred at write-time from
  which time fields you supplied.
- **State** — derived from `switch` entries: at any past instant we can
  reconstruct a field's value. `datum now` consults this.
- **Slug vs. hid** — `slug` is the human-readable id (`mood:2026-04-29T09:15Z`).
  `hid` is the short hash you see in `datum tail`; both resolve to the
  same entry.
- **Intent** — the original argv is captured and replayed under a
  pinned `parser_version`, so a future parser fix can retroactively
  correct old entries without losing what the user actually typed
  (see `da-ydg`).

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

- `add` — Add a new entry (timeless data)
- `occur` — Record a point-in-time event
- `start` — Begin a duration-tracked activity
- `stop` — End a duration-tracked activity (alias: end)
- `switch` — Transition a field to a new state
- `track` — Track a field in the now view
- `ignore` — Ignore a field in the now view

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
- `now` — Show current state of tracked fields
- `get` — Print a single entry by id or slug
- `grep` — Regex search over entry JSON content
- `history` — Show the full modification trail for an entry
- `why` — Explain the intent behind an entry

## RENDER

- `dayview` — Render a dayview SVG (or dump layout JSON)
- `nowview` — Render a nowview — terminal block art by default, SVG with -o

## ADMIN

- `spec` — Manage field specs (schemas)
- `alias` — Manage aliases
- `migrate` — Schema migrations + legacy datum import
- `backup` — Dump live entries to JSONL / tar.gz
- `restore` — Re-import entries from a backup
- `check` — Run database consistency checks
- `setup` — Initialize or migrate the DB schema
- `db` — DB-lifecycle subcommands (purge, …)
- `info` — Show health/metadata for DB(s)
- `config` — Show or manage user configuration

## VIEWS / SYNC

- `count` — Count entries (optionally filtered/grouped/since-bounded)
- `ids` — List entry ids (newest-first by default)
- `sync` — Push/pull changesets, manage auth & keys
- `conflicts` — Inspect and resolve sync conflicts

## Global Options

- `--db <NAME_OR_PATH>` — Override the active DB
- `--show <MODE>` — default|none|minimal|standard|format|all
- `-A, --show-all` — Shorthand for --show all
- `-o, --format-string` — %field%-templated output string
- `-h, --help` — Show this help
- `-V, --version` — Show version
<!-- END AUTOGEN: COMMANDS -->

## Sync

Sync is opt-in and end-to-end encrypted. The relay server stores opaque
ciphertext blobs and the metadata (device id, sequence number) it needs
to fan changesets out to your other devices. It cannot read your data.

- `crates/datum-sync-client/` — wire protocol, crypto, HTTP client.
- `crates/datum-sync-server/` — Rust + axum relay.
- `crates/datum-sync-agent/` — background daemon (planned).

Per-crate READMEs land in **da-docs.3**.

## Configuration

`datum` is configured through (in order of precedence):

1. CLI flags (`--db`, `--show`, ...).
2. Environment variables.
3. `datumrc.yml` in `$DATUM_CONFIG_HOME` (or the OS default config dir).

Notable env vars:

| Variable                | Purpose                                                |
|-------------------------|--------------------------------------------------------|
| `DATUM_DB`              | Path or registry name of the active database.          |
| `DATUM_HOME`            | Override the data directory (default: OS data dir).    |
| `DATUM_CONFIG`          | Path to a specific `datumrc.yml`.                      |
| `DATUM_CONFIG_HOME`     | Override the directory `datumrc.yml` lives in.         |
| `DATUM_TIMEZONE`        | Default IANA timezone for new entries.                 |
| `DATUM_DEFAULT_FIELD`   | Field used when one is omitted from the argv.          |
| `DATUM_DEVICE_ID`       | Override the per-device id stamped on entries.         |
| `DATUM_EDITOR`          | Editor for `datum edit` (falls back to `EDITOR`).      |
| `DATUM_COLOR`           | `0` disables colour; otherwise auto-detect from TTY.   |

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
