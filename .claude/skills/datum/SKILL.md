---
name: datum
description: Log and review personal life-data with the `datum` CLI in this repo. Use when recording events, states, or durations (the verbs occur / start / stop / switch / add) or reading them back (tail / now / get / grep / dayview). Covers time-flags for backdating, choosing a database, and the data model.
---

# Using datum

`datum` is a local-first life-tracker: every entry is one row in a local SQLite
database, and logging is near-instant. You record *moments and states*, then read
them back as lists, current state, or visual timelines. `datum --help` and
`datum <command> --help` document the full surface.

## Logging — pick the verb by what you're recording

```bash
datum occur coffee                  # a point-in-time event (no duration)
datum switch mood happy             # a field's new state (persists until changed)
datum switch weight 75.2kg          # state + structured data (units parse to {"v":75.2,"u":"kg"})
datum start work project=writing    # begin a duration-tracked activity
datum stop work                     # end the open one (alias: end)
datum add person name=Ada           # timeless data (no occur-time)
```

`track` / `ignore` control whether a field appears in `datum now`; `flag` marks a
field to come back to. Structured data is `key=value`; a trailing unit on a number
is parsed automatically.

## Backdating / shifting time

An entry logs *now* unless you place it:

- `-y` = 1 day ago, `-yy` = 2 (stackable); `-d -1` = yesterday, `-d +1` = tomorrow.
- `-q` = 5 min ago (stackable); `-t -8` = 8 min ago; `-t 1513` = 15:13 today; `-t 9:15am`.
- `--dur 30m` (also `1:30`, `2h`, ISO `PT30M`) sets a duration; combine with `-t` to
  derive the missing end/start.

## Reading it back

```bash
datum tail                          # recent entries
datum tail mood                     # filtered to one field
datum now                           # current state of tracked fields + a terminal timeline band
datum get <id-or-slug>              # one entry by id or slug
datum grep <regex>                  # regex search over entry JSON
datum count --group-by field        # counts, optionally grouped / --since-bounded
datum dayview                       # render a day as an SVG timeline ("data as art")
```

## The data model (essentials)

An **entry** has a `field` (what is tracked), a `state` (JSON value), an optional
ISO-8601 `dur`, an `occur_time`, and a `slug` (`field:time` — the human-readable id;
the short `hid` in `datum tail` resolves to the same entry). You choose the CLI verb;
datum *derives* the storage classifier (point / switch / block / block-switch /
point-switch) from the (state, dur) it produces.

## Choosing a database

Entries go to the default database unless overridden — `datum --db <name-or-path> …`
or the `DATUM_DB` environment variable. All reads (`tail`, `now`, …) respect the same
selection. Use a throwaway db while experimenting: `datum --db /tmp/scratch.db occur test`.

## Handy

- **Aliases** pre-bake commands: `datum alias set vbp "switch body vertical , occur caffeine"`,
  then `datum vbp` runs the body.
- A whitespace-bounded `,` token logs **multiple entries atomically** with one shared
  timestamp: `datum switch body vertical , occur caffeine`.
- `datum --man <command>` prints a man page to stdout; `datum spec set <field> …`
  constrains a field's schema (allowed states, colors, id template).
