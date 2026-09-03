---
name: datum
description: Log and review personal life-data with the `datum` CLI in this repo. Use when recording events, states, or durations (the verbs occur / start / stop / switch / note / add) or reading them back (tail / now / get / grep / dayview). Covers time-flags for backdating, choosing a database, and the data model.
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
datum note idea spark=svg-timeline  # timeless jot, auto-id'd on create_time
datum add person -K name name=Ada   # timeless keyed record (needs an explicit id)
```

`track` / `ignore` control whether a field appears in `datum now`; `flag` marks a
field to come back to. Structured data is `key=value`; a trailing unit on a number
is parsed automatically.

### note vs add — timeless data, split by identity (when vs what)

Both `note` and `add` record **timeless** data (no `occur_time`); they differ in
how the entry is identified:

- **`note` = "record this moment, won't look it up."** Identified by *when* —
  auto-ids on its create_time, so its slug is `field:c<create_time>` and many
  notes per field coexist. No id needed. Use it to update a field without
  claiming an occurrence.
- **`add` = "register this keyed thing, will look it up."** Identified by *what* —
  it **requires** an explicit id so you can reference it later. Give one with
  `-K <key>` (derive the slug from a data key), `--id <value>` (set it directly,
  templates like `--id %field%__%srcLang%` allowed), or the `id=<value>`
  positional. A bare `add field` with no id errors descriptively.

`-T` / `--no-timestamp` converts any occurred verb into **note-data**: the data
still lands, but no occurrence is logged and the entry gets a create_time id.
Canonical use — *chore-shirk*: advance a recurring chore's schedule without
claiming you did it.

```bash
datum occur chore -T next_date=+3   # bump the next-due date; logs no occurrence
```

`-T` (drop the time entirely → timeless) is distinct from `-D` / `--full-day`
(keep a **date**, drop the clock time → a date-only, still time-anchored entry).

## Backdating / shifting time

An entry logs *now* unless you place it:

- `-y` = 1 day ago, `-yy` = 2 (stackable); `-d -1` = yesterday, `-d +1` = tomorrow.
- `-q` = 5 min ago (stackable); `-t -8` = 8 min ago; `-t 1513` = 15:13 today; `-t 9:15am`.
- A duration is a trailing positional (`datum end work 30m`) or `dur=30m` — also
  `1:30`, `2h`, ISO `PT30M`. There is no `--dur` flag. Combined with `-t`, the block
  starts at `-t` and runs forward: `datum end work 12m -t 14:00` is 14:00–14:12.

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
ISO-8601 `dur`, an `occur_time`, and a `slug` — the human-readable id, `field:<time>`
for a time-anchored entry or `field:c<create_time>` for a timeless `note`/`-T` one
(the short `hid` in `datum tail` resolves to the same entry). You choose the CLI verb;
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
